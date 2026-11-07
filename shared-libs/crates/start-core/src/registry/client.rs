use std::fmt;
use std::str::FromStr;

use http::{HeaderMap, HeaderValue};
use reqwest::Client;
use rpc_toolkit::yajrc::RpcError;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::middleware::auth::signature::call_remote_with_client;
use crate::prelude::*;
use crate::registry::device_info::{DEVICE_INFO_HEADER, DeviceInfo};

pub const REGISTRY_CLIENT_HEADER: &str = "X-Start9-Registry-Client";

const ED25519PH: &str = "ed25519ph";

/// The operating system whose releases a registry indexes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
#[value(rename_all = "lowercase")]
pub enum Product {
    #[default]
    StartOs,
    StartWrt,
}

impl Product {
    fn as_str(self) -> &'static str {
        match self {
            Self::StartOs => "startos",
            Self::StartWrt => "startwrt",
        }
    }
}

impl fmt::Display for Product {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::StartOs => "StartOS",
            Self::StartWrt => "StartWRT",
        })
    }
}

impl FromStr for Product {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "startos" => Ok(Self::StartOs),
            "startwrt" => Ok(Self::StartWrt),
            _ => Err(Error::new(
                eyre!("{}", t!("registry.client.unknown-product", product = s)),
                ErrorKind::InvalidRequest,
            )),
        }
    }
}

/// What a registry client declares about itself in [`REGISTRY_CLIENT_HEADER`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RegistryClient {
    /// The operating system asking for its own releases. Unset for a client of any registry.
    pub product: Option<Product>,
    /// Reads Ed25519ph-labelled and pure Ed25519 signatures.
    pub reads_ed25519ph: bool,
}

impl RegistryClient {
    /// A client built from this source tree.
    pub const fn current(product: Option<Product>) -> Self {
        Self {
            product,
            reads_ed25519ph: true,
        }
    }

    pub fn to_header_value(&self) -> HeaderValue {
        let mut ser = form_urlencoded::Serializer::new(String::new());
        if let Some(product) = self.product {
            ser.append_pair("product", product.as_str());
        }
        if self.reads_ed25519ph {
            ser.append_pair("cap", ED25519PH);
        }
        HeaderValue::from_str(&ser.finish()).unwrap()
    }

    /// Unknown keys and capabilities are ignored.
    pub fn from_header_value(header: &HeaderValue) -> Result<Self, Error> {
        let mut client = Self::default();
        for (key, value) in form_urlencoded::parse(header.as_bytes()) {
            match &*key {
                "product" => client.product = Some(value.parse()?),
                "cap" if value == ED25519PH => client.reads_ed25519ph = true,
                _ => (),
            }
        }
        Ok(client)
    }

    pub fn insert_into(&self, headers: &mut HeaderMap) {
        headers.insert(REGISTRY_CLIENT_HEADER, self.to_header_value());
    }
}

/// Calls a registry method unsigned, as an OS asking for its own releases.
pub async fn call(
    client: Client,
    mut registry: Url,
    product: Product,
    device_info: Option<&DeviceInfo>,
    method: &str,
    mut params: Value,
) -> Result<Value, RpcError> {
    super::compat::pem_wire(&mut params);
    registry
        .path_segments_mut()
        .map_err(|_| Error::new(eyre!("cannot extend URL path"), ErrorKind::ParseUrl))?
        .push("rpc")
        .push("v0");
    let mut headers = HeaderMap::new();
    RegistryClient::current(Some(product)).insert_into(&mut headers);
    if let Some(device_info) = device_info {
        headers.insert(DEVICE_INFO_HEADER, device_info.to_header_value());
    }
    call_remote_with_client(None, client, registry, headers, method, params).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_round_trips() {
        for client in [
            RegistryClient::default(),
            RegistryClient::current(None),
            RegistryClient::current(Some(Product::StartOs)),
            RegistryClient::current(Some(Product::StartWrt)),
        ] {
            assert_eq!(
                RegistryClient::from_header_value(&client.to_header_value()).unwrap(),
                client
            );
        }
    }

    #[test]
    fn ignores_unknown_keys_and_capabilities() {
        let client = RegistryClient::from_header_value(&HeaderValue::from_static(
            "product=startwrt&cap=future&other=1",
        ))
        .unwrap();
        assert_eq!(
            client,
            RegistryClient {
                product: Some(Product::StartWrt),
                reads_ed25519ph: false,
            }
        );
    }

    #[tokio::test]
    async fn unsigned_calls_keep_pem_parameters_and_client_headers() {
        use axum::Json;
        use axum::routing::post;
        use imbl_value::json;
        use tokio::net::TcpListener;

        use crate::sign::AnyVerifyingKey;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let registry: Url = format!("http://{}", listener.local_addr().unwrap())
            .parse()
            .unwrap();
        let app = axum::Router::new().route(
            "/rpc/v0",
            post(|headers: HeaderMap, Json(request): Json<Value>| async move {
                Json(json!({
                    "jsonrpc": "2.0",
                    "id": request["id"],
                    "result": {
                        "method": request["method"],
                        "params": request["params"],
                        "client": headers[REGISTRY_CLIENT_HEADER].to_str().unwrap(),
                        "deviceInfo": headers.get(DEVICE_INFO_HEADER).map(|h| h.to_str().unwrap()),
                    },
                }))
            }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let key = AnyVerifyingKey::Ed25519(
            ed25519_dalek::SigningKey::from_bytes(&[13; 32]).verifying_key(),
        );
        let params = json!({ "signer": key });
        assert!(!params["signer"].as_str().unwrap().contains("BEGIN"));
        for (product, header) in [
            (
                Product::StartOs,
                "os.version=0.4.0.5&os.compat=*&os.platform=x86_64",
            ),
            (
                Product::StartWrt,
                "os.version=1.3.1&os.compat=^1.2.0&os.platform=spacemit,k1-x",
            ),
        ] {
            let device_info =
                DeviceInfo::from_header_value(&HeaderValue::from_static(header)).unwrap();
            for device_info in [None, Some(&device_info)] {
                let response = call(
                    Client::new(),
                    registry.clone(),
                    product,
                    device_info,
                    "signer.get",
                    params.clone(),
                )
                .await
                .unwrap();
                assert_eq!(response["method"].as_str(), Some("signer.get"));
                assert_eq!(
                    response["params"]["signer"].as_str(),
                    Some(key.to_string().as_str())
                );
                let client = RegistryClient::from_header_value(
                    &HeaderValue::from_str(response["client"].as_str().unwrap()).unwrap(),
                )
                .unwrap();
                assert_eq!(client, RegistryClient::current(Some(product)));
                assert_eq!(
                    response["deviceInfo"],
                    to_value(
                        &device_info.map(|d| d.to_header_value().to_str().unwrap().to_owned())
                    )
                    .unwrap(),
                );
            }
        }
        server.abort();
    }

    #[test]
    fn rejects_an_unknown_product() {
        assert!(
            RegistryClient::from_header_value(&HeaderValue::from_static("product=other")).is_err()
        );
    }
}
