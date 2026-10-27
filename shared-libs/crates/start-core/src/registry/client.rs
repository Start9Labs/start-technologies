use std::fmt;
use std::str::FromStr;

use http::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};

use crate::prelude::*;

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

    #[test]
    fn rejects_an_unknown_product() {
        assert!(
            RegistryClient::from_header_value(&HeaderValue::from_static("product=other")).is_err()
        );
    }
}
