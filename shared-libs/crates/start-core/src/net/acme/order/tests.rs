use std::sync::{Arc, Mutex};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use openssl::asn1::Asn1Time;
use openssl::hash::MessageDigest;
use openssl::x509::X509Req;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::*;
use crate::db::DbAccess;

#[derive(Default, Serialize, Deserialize, HasModel)]
#[model = "Model<Self>"]
struct TestDatabase {
    acme: AcmeCertStore,
}

impl DbAccess<AcmeCertStore> for TestDatabase {
    fn access(db: &Model<Self>) -> &Model<AcmeCertStore> {
        db.as_acme()
    }
}

impl DbAccessMut<AcmeCertStore> for TestDatabase {
    fn access_mut(db: &mut Model<Self>) -> &mut Model<AcmeCertStore> {
        db.as_acme_mut()
    }
}

async fn database() -> (tempfile::TempDir, TypedPatchDb<TestDatabase>) {
    let temp = tempfile::tempdir().unwrap();
    let db = TypedPatchDb::load_unchecked(
        patch_db::PatchDb::open(temp.path().join("db"))
            .await
            .unwrap(),
    );
    db.mutate(|db: &mut Model<TestDatabase>| db.ser(&TestDatabase::default()))
        .await
        .result
        .unwrap();
    (temp, db)
}

#[derive(Clone, Copy)]
enum Scenario {
    Success,
    Conflict,
    Processing,
    Rejected,
    RateLimited,
}

#[derive(Clone)]
struct MockCa(Arc<Mutex<MockState>>);

struct MockState {
    scenario: Scenario,
    triggers: usize,
    polls: usize,
    finalized: bool,
    account_keys: Vec<Value>,
    contacts: Vec<Value>,
    challenge_available: bool,
    issuer: openssl::pkey::PKey<openssl::pkey::Private>,
    issuer_cert: X509,
    certificate: String,
}

impl MockCa {
    fn new(scenario: Scenario) -> Self {
        let key = KeyPair::generate().unwrap();
        let mut params = CertificateParams::default();
        params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        let cert = params.self_signed(&key).unwrap();
        Self(Arc::new(Mutex::new(MockState {
            scenario,
            triggers: 0,
            polls: 0,
            finalized: false,
            account_keys: Vec::new(),
            contacts: Vec::new(),
            challenge_available: false,
            issuer: PKey::private_key_from_der(&key.serialize_der()).unwrap(),
            issuer_cert: X509::from_der(cert.der()).unwrap(),
            certificate: String::new(),
        })))
    }
}

fn response(status: u16, body: String) -> BytesResponse {
    let mut response = http::Response::builder()
        .status(status)
        .header("Replay-Nonce", "test-nonce")
        .header("Location", "https://acme.test/order");
    if status == 429 {
        response = response.header("Retry-After", "3600");
    }
    response
        .body(http_body_util::Full::new(Bytes::from(body)))
        .unwrap()
        .into()
}

fn sign_request(request: &X509Req, issuer: &MockState, days: i64) -> X509 {
    let mut builder = X509::builder().unwrap();
    builder.set_version(2).unwrap();
    builder
        .set_serial_number(
            &openssl::bn::BigNum::from_u32(1)
                .unwrap()
                .to_asn1_integer()
                .unwrap(),
        )
        .unwrap();
    builder.set_subject_name(request.subject_name()).unwrap();
    builder
        .set_issuer_name(issuer.issuer_cert.subject_name())
        .unwrap();
    builder.set_pubkey(&request.public_key().unwrap()).unwrap();
    let now = chrono::Utc::now().timestamp();
    builder
        .set_not_before(&Asn1Time::from_unix(now - 30 * 86400).unwrap())
        .unwrap();
    builder
        .set_not_after(&Asn1Time::from_unix(now + days * 86400).unwrap())
        .unwrap();
    for extension in request.extensions().unwrap() {
        builder.append_extension(extension).unwrap();
    }
    builder
        .sign(&issuer.issuer, MessageDigest::sha256())
        .unwrap();
    builder.build()
}

fn order_state(status: &str) -> Value {
    json!({"status":status,"identifiers":[{"type":"dns","value":"example.test"}],
        "authorizations":["https://acme.test/auth"],"finalize":"https://acme.test/finalize",
        "certificate":"https://acme.test/cert"})
}

impl HttpClient for MockCa {
    fn request(
        &self,
        request: http::Request<BodyWrapper<Bytes>>,
    ) -> Pin<Box<dyn Future<Output = Result<BytesResponse, AcmeError>> + Send>> {
        let state = self.0.clone();
        Box::pin(async move {
            let path = request.uri().path().to_owned();
            let body = request.into_body().collect().await.unwrap().to_bytes();
            let jws: Value = if body.is_empty() {
                Value::Null
            } else {
                serde_json::from_slice(&body).unwrap()
            };
            let payload = jws["payload"]
                .as_str()
                .map(|p| URL_SAFE_NO_PAD.decode(p).unwrap())
                .unwrap_or_default();
            let payload: Value = if payload.is_empty() {
                Value::Null
            } else {
                serde_json::from_slice(&payload).unwrap()
            };
            let mut state = state.lock().unwrap();
            let result = match path.as_str() {
                "/directory" => {
                    json!({"newNonce":"https://acme.test/nonce","newAccount":"https://acme.test/new-account","newOrder":"https://acme.test/new-order"})
                }
                "/nonce" => return Ok(response(200, String::new())),
                "/new-account" => {
                    let protected: Value = serde_json::from_slice(
                        &URL_SAFE_NO_PAD
                            .decode(jws["protected"].as_str().unwrap())
                            .unwrap(),
                    )
                    .unwrap();
                    state.account_keys.push(protected["jwk"].clone());
                    json!({"status":"valid"})
                }
                "/order" if payload.get("contact").is_some() => {
                    state.contacts.push(payload["contact"].clone());
                    json!({"status":"valid"})
                }
                "/new-order" => {
                    if matches!(state.scenario, Scenario::RateLimited) {
                        return Ok(response(429, json!({"type":"urn:ietf:params:acme:error:rateLimited","detail":"Please visit: https://portal.letsencrypt.org/sfe/v1/unpause?jwt=test-token","status":429}).to_string()));
                    }
                    order_state("pending")
                }
                "/auth" => {
                    let invalid = matches!(state.scenario, Scenario::Rejected) && state.polls > 0;
                    let mut challenge = json!({"type":"tls-alpn-01","url":"https://acme.test/challenge","token":"test-token","status":"pending"});
                    if matches!(state.scenario, Scenario::Processing) {
                        challenge["status"] = json!("processing");
                    }
                    if invalid {
                        challenge["status"] = json!("invalid");
                        challenge["error"] = json!({"type":"urn:ietf:params:acme:error:unauthorized","detail":"The challenge certificate did not match"});
                    }
                    json!({"identifier":{"type":"dns","value":"example.test"},"status":if invalid {"invalid"} else {"pending"},"challenges":[challenge]})
                }
                "/challenge" => {
                    assert!(
                        state.challenge_available,
                        "challenge must be installed before triggering validation"
                    );
                    state.triggers += 1;
                    if matches!(state.scenario, Scenario::Conflict) {
                        return Ok(response(
                            409,
                            json!({"type":"urn:ietf:params:acme:error:conflict","status":409})
                                .to_string(),
                        ));
                    }
                    json!({"type":"tls-alpn-01","url":"https://acme.test/challenge","token":"test-token","status":"processing"})
                }
                "/order" => {
                    state.polls += 1;
                    order_state(if state.finalized {
                        "valid"
                    } else if matches!(state.scenario, Scenario::Rejected) {
                        "invalid"
                    } else if state.polls < 3 {
                        "pending"
                    } else {
                        "ready"
                    })
                }
                "/finalize" => {
                    let der = URL_SAFE_NO_PAD
                        .decode(payload["csr"].as_str().unwrap())
                        .unwrap();
                    let request = X509Req::from_der(&der).unwrap();
                    assert!(request.verify(&request.public_key().unwrap()).unwrap());
                    assert_eq!(request.subject_name().entries().count(), 0);
                    let certificate = sign_request(&request, &state, 90);
                    state.certificate = String::from_utf8(certificate.to_pem().unwrap()).unwrap()
                        + &String::from_utf8(state.issuer_cert.to_pem().unwrap()).unwrap();
                    state.finalized = true;
                    order_state("processing")
                }
                "/cert" => return Ok(response(200, state.certificate.clone())),
                _ => panic!("unexpected ACME request: {path} {payload}"),
            };
            Ok(response(200, result.to_string()))
        })
    }
}

fn sans() -> BTreeSet<InternedString> {
    BTreeSet::from(["example.test".into()])
}

#[tokio::test(start_paused = true)]
async fn issuance_polls_without_retriggering_and_persists_the_chain() {
    for scenario in [Scenario::Success, Scenario::Conflict, Scenario::Processing] {
        let (_temp, db) = database().await;
        let cache = AcmeCertCache(&db);
        let ca = MockCa::new(scenario);
        let ready = Arc::new(Mutex::new(false));
        let observed = ready.clone();
        let challenge_ca = ca.clone();
        let cert = order(
            move |identifier, cert| {
                assert!(matches!(identifier, Identifier::Dns(name) if name == "example.test"));
                assert_eq!(cert.cert.len(), 1);
                *observed.lock().unwrap() = true;
                challenge_ca.0.lock().unwrap().challenge_available = true;
                Ok(())
            },
            "https://acme.test/directory",
            &sans(),
            &cache,
            &["mailto:admin@example.test".into()],
            &provider(),
            Box::new(ca.clone()),
        )
        .await
        .unwrap();
        assert!(*ready.lock().unwrap());
        cert.keys_match().unwrap();
        assert_eq!(cert.cert.len(), 2);
        let peek = db.peek().await;
        let stored = peek
            .as_acme()
            .as_certs()
            .as_idx(&"https://acme.test/directory".parse().unwrap())
            .unwrap()
            .as_idx(JsonKey::new_ref(&sans()))
            .unwrap()
            .de()
            .unwrap();
        assert_eq!(stored.fullchain.len(), 2);
        assert_eq!(
            stored.fullchain[0].0.to_der().unwrap(),
            cert.cert[0].as_ref()
        );
        let state = ca.0.lock().unwrap();
        assert_eq!(
            state.triggers,
            if matches!(scenario, Scenario::Processing) {
                0
            } else {
                1
            }
        );
        assert!(state.polls >= 4);
        assert!(stored.fullchain[0].0.verify(&state.issuer).unwrap());
        assert_eq!(state.contacts, vec![json!(["mailto:admin@example.test"])]);
    }
}

#[tokio::test(start_paused = true)]
async fn rejected_authorization_surfaces_the_cas_reason() {
    let (_temp, db) = database().await;
    let ca = MockCa::new(Scenario::Rejected);
    let challenge_ca = ca.clone();
    let error = order(
        move |_, _| {
            challenge_ca.0.lock().unwrap().challenge_available = true;
            Ok(())
        },
        "https://acme.test/directory",
        &sans(),
        &AcmeCertCache(&db),
        &[],
        &provider(),
        Box::new(ca.clone()),
    )
    .await
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("The challenge certificate did not match")
    );
    assert_eq!(ca.0.lock().unwrap().triggers, 1);
}

#[tokio::test]
async fn rate_limit_keeps_the_deadline_and_unpause_link() {
    let (_temp, db) = database().await;
    let error = order(
        |_, _| panic!("rate-limited orders have no challenge"),
        "https://acme.test/directory",
        &sans(),
        &AcmeCertCache(&db),
        &[],
        &provider(),
        Box::new(MockCa::new(Scenario::RateLimited)),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        error,
        AcmeError::RateLimited {
            retry_after: Some(_),
            ..
        }
    ));
    assert!(
        error
            .to_string()
            .contains("https://portal.letsencrypt.org/sfe/v1/unpause?jwt=test-token")
    );
}

#[tokio::test]
async fn account_keys_survive_reloading_the_existing_database_format() {
    let (_temp, db) = database().await;
    let cache = AcmeCertCache(&db);
    let contact = vec!["mailto:admin@example.test".to_owned()];
    let ca = MockCa::new(Scenario::Success);
    load_account(
        &cache,
        "https://acme.test/directory",
        &contact,
        Box::new(ca.clone()),
    )
    .await
    .unwrap();
    let first = cache.read_account(&[&contact[0]]).await.unwrap().unwrap();
    load_account(
        &cache,
        "https://acme.test/directory",
        &contact,
        Box::new(ca.clone()),
    )
    .await
    .unwrap();
    assert_eq!(
        cache.read_account(&[&contact[0]]).await.unwrap().unwrap(),
        first
    );
    let state = ca.0.lock().unwrap();
    assert_eq!(state.account_keys.len(), 2);
    assert_ne!(state.account_keys[0], Value::Null);
    assert_eq!(state.account_keys[0], state.account_keys[1]);
}

impl crate::db::DbAccessByKey<crate::db::model::public::AcmeSettings> for TestDatabase {
    type Key<'a> = &'a super::super::AcmeProvider;
    fn access_by_key<'a>(
        _: &'a Model<Self>,
        _: Self::Key<'_>,
    ) -> Option<&'a Model<crate::db::model::public::AcmeSettings>> {
        None
    }
}

#[derive(Clone)]
struct ConfiguredProvider;
impl super::super::GetAcmeProvider for ConfiguredProvider {
    async fn get_provider<'a, 'b: 'a>(
        &'b self,
        _: &'a BTreeSet<InternedString>,
    ) -> Option<impl AsRef<super::super::AcmeProvider> + Send + 'b> {
        Some(
            "https://acme.test/directory"
                .parse::<super::super::AcmeProvider>()
                .unwrap(),
        )
    }
}

#[tokio::test]
async fn cached_certificates_keep_working_after_provider_configuration_is_removed() {
    use super::super::{AcmeTlsHandler, OrderEntry};
    use crate::util::sync::{SyncMutex, Watch};
    for days in [90, 5, -1] {
        let (_temp, db) = database().await;
        let key = KeyPair::generate().unwrap();
        let request = CertificateParams::new(vec!["example.test".to_owned()])
            .unwrap()
            .serialize_request(&key)
            .unwrap();
        let ca = MockCa::new(Scenario::Success);
        let certificate = sign_request(
            &X509Req::from_der(request.der()).unwrap(),
            &ca.0.lock().unwrap(),
            days,
        );
        let cert = AcmeCert {
            key: Der64(PKey::private_key_from_pkcs8(&key.serialize_der()).unwrap()),
            fullchain: vec![Pem(certificate)],
        };
        AcmeCertCache(&db)
            .write_certificate(&sans(), "https://acme.test/directory", &cert)
            .await
            .unwrap();
        let handler = AcmeTlsHandler {
            db,
            acme_cache: Arc::new(SyncMutex::new(Default::default())),
            crypto_provider: Arc::new(provider()),
            get_provider: ConfiguredProvider,
            in_progress: Watch::new([(sans(), OrderEntry::default())].into_iter().collect()),
            report_failure: None,
        };
        assert_eq!(handler.get_cert(&sans()).await.is_some(), days > 0);
        if days == 90 {
            assert!(handler.in_progress.peek(|entries| entries.is_empty()));
        }
    }
}

#[tokio::test]
async fn native_tls_transport_checks_private_ca_trust_and_preserves_response_metadata() {
    use hyper::service::service_fn;
    use hyper_util::rt::TokioIo;
    use tokio::net::TcpListener;
    use tokio_rustls::TlsAcceptor;
    use tokio_rustls::rustls::ServerConfig;

    let mut ca_params = CertificateParams::default();
    ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    ca_params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "ACME transport test root");
    let issuer =
        rcgen::CertifiedIssuer::self_signed(ca_params, KeyPair::generate().unwrap()).unwrap();
    let key = KeyPair::generate().unwrap();
    let cert = CertificateParams::new(vec!["localhost".to_owned()])
        .unwrap()
        .signed_by(&key, &issuer)
        .unwrap();
    let config = ServerConfig::builder_with_provider(Arc::new(provider()))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![cert.der().clone(), issuer.der().clone()],
            PrivatePkcs8KeyDer::from(key.serialize_der()).into(),
        )
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "https://localhost:{}/new-order",
        listener.local_addr().unwrap().port()
    );
    let server = tokio::spawn(async move {
        let acceptor = TlsAcceptor::from(Arc::new(config));
        for _ in 0..2 {
            let (stream, _) = listener.accept().await.unwrap();
            let Ok(stream) = acceptor.accept(stream).await else {
                continue;
            };
            hyper::server::conn::http1::Builder::new()
                .keep_alive(false)
                .serve_connection(
                    TokioIo::new(stream),
                    service_fn(|request: http::Request<hyper::body::Incoming>| async move {
                        assert_eq!(request.method(), http::Method::POST);
                        assert_eq!(request.uri().path(), "/new-order");
                        assert_eq!(request.headers()["content-type"], "application/jose+json");
                        assert_eq!(
                            request.into_body().collect().await.unwrap().to_bytes(),
                            "signed request"
                        );
                        Ok::<_, std::convert::Infallible>(
                            http::Response::builder()
                                .status(429)
                                .header("Retry-After", "120")
                                .body(http_body_util::Full::new(Bytes::from_static(
                                    b"problem body",
                                )))
                                .unwrap(),
                        )
                    }),
                )
                .await
                .unwrap();
        }
    });
    let request = || {
        http::Request::builder()
            .method("POST")
            .uri(&url)
            .header("content-type", "application/jose+json")
            .body(BodyWrapper::from(b"signed request".to_vec()))
            .unwrap()
    };
    assert!(
        AcmeHttpClient::new()
            .unwrap()
            .request(request())
            .await
            .is_err()
    );
    let client = reqwest::Client::builder()
        .use_native_tls()
        .no_proxy()
        .add_root_certificate(reqwest::Certificate::from_pem(issuer.pem().as_bytes()).unwrap())
        .build()
        .unwrap();
    let response = AcmeHttpClient(client).request(request()).await.unwrap();
    assert_eq!(response.parts.status, http::StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response.parts.headers["Retry-After"], "120");
    let mut body = response.body;
    assert_eq!(body.into_bytes().await.unwrap(), "problem body");
    server.await.unwrap();
}

struct FailingHttp;
impl HttpClient for FailingHttp {
    fn request(
        &self,
        _: http::Request<BodyWrapper<Bytes>>,
    ) -> Pin<Box<dyn Future<Output = Result<BytesResponse, AcmeError>> + Send>> {
        Box::pin(async {
            Err(AcmeError::Other(
                std::io::Error::other("offline test fixture").into(),
            ))
        })
    }
}

#[tokio::test]
async fn new_account_key_is_persisted_before_network_failure() {
    let (_temp, db) = database().await;
    let cache = AcmeCertCache(&db);
    assert!(
        load_account(
            &cache,
            "https://acme.test/directory",
            &[],
            Box::new(FailingHttp)
        )
        .await
        .is_err()
    );
    let first = cache.read_account(&[]).await.unwrap().unwrap();
    assert!(
        load_account(
            &cache,
            "https://acme.test/directory",
            &[],
            Box::new(FailingHttp)
        )
        .await
        .is_err()
    );
    assert_eq!(cache.read_account(&[]).await.unwrap().unwrap(), first);
}

fn provider() -> CryptoProvider {
    tokio_rustls::rustls::crypto::ring::default_provider()
}

#[test]
fn challenge_certificate_contains_the_critical_digest_and_matching_key() {
    let digest = [42; 32];
    let certified = challenge_cert(
        &Identifier::Dns("example.test".into()),
        &digest,
        &provider(),
    )
    .unwrap();
    let cert = X509::from_der(certified.cert[0].as_ref()).unwrap();
    assert_eq!(
        certified.key.public_key().unwrap().as_ref(),
        cert.public_key().unwrap().public_key_to_der().unwrap()
    );
    assert_eq!(
        cert.subject_alt_names().unwrap()[0].dnsname(),
        Some("example.test")
    );
    let text = String::from_utf8(cert.to_text().unwrap()).unwrap();
    assert!(text.contains("1.3.6.1.5.5.7.1.31: critical"));
    let expected = [vec![4, 32], digest.to_vec()].concat();
    assert!(
        certified.cert[0]
            .windows(expected.len())
            .any(|bytes| bytes == expected)
    );
}

#[test]
fn challenge_certificate_retains_ip_sans() {
    let ip = "192.0.2.1".parse().unwrap();
    let certified = challenge_cert(&Identifier::Ip(ip), &[1; 32], &provider()).unwrap();
    let cert = X509::from_der(certified.cert[0].as_ref()).unwrap();
    assert_eq!(
        cert.subject_alt_names().unwrap()[0].ipaddress(),
        Some(&[192, 0, 2, 1][..])
    );
}
