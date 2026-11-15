use super::*;

#[tokio::test]
#[ignore = "Reads live registries supplied in REGISTRY_LIVE_URLS"]
async fn live_legacy_registries_discover_and_populate_both_catalogs() {
    use fast_socks5::server::{Socks5ServerProtocol, run_tcp_proxy};

    use crate::context::rpc::ReloadableHttpClient;
    use crate::registry::context::call;
    use crate::sign::{AnySigningKey, Signer};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client =
        ReloadableHttpClient::new(format!("socks5h://{}", listener.local_addr().unwrap())).unwrap();
    let _proxy: NonDetachingJoinHandle<()> = tokio::spawn(async move {
        let mut connections = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                stream = listener.accept() => {
                    let (stream, _) = stream.unwrap();
                    connections.spawn(async move {
                        let (protocol, _, target) = Socks5ServerProtocol::accept_no_auth(stream).await.unwrap().read_command().await.unwrap();
                        let target = target.resolve_dns().await.unwrap();
                        run_tcp_proxy(protocol, &target, Duration::from_secs(20), true).await.unwrap();
                    });
                }
                result = connections.join_next(), if !connections.is_empty() => { result.unwrap().unwrap(); }
            }
        }
    }).into();
    let signer: Signer =
        AnySigningKey::Ed25519(ed25519_dalek::SigningKey::from_bytes(&[81; 32])).into();
    for raw in std::env::var("REGISTRY_LIVE_URLS")
        .expect("REGISTRY_LIVE_URLS required")
        .split_whitespace()
    {
        let registry = registry_url(raw).unwrap();
        let info = call(
            &client,
            &signer,
            &registry,
            http::HeaderMap::new(),
            "info",
            json!({}),
        )
        .await
        .unwrap();
        assert!(info["name"].as_str().is_some(), "{registry}: {info:?}");
        let temp = tempfile::tempdir().unwrap();
        let db = TypedPatchDb::<Database>::load_unchecked(
            PatchDb::open(temp.path().join("db")).await.unwrap(),
        );
        let mut initial = database();
        initial
            .as_public_mut()
            .as_server_info_mut()
            .mutate(|info| {
                info.registries = from_value(json!({registry.as_str(): "Live legacy"}))?;
                info.os_registry = registry.clone();
                Ok::<_, Error>(())
            })
            .unwrap();
        db.put(&ROOT, initial.as_value()).await.unwrap();
        let subscriptions = BTreeSet::from([
            Subscription::Index(registry.clone()),
            Subscription::Os(registry.clone()),
        ]);
        super::super::polling::poll(&db, &subscriptions, &device(), &client, &signer)
            .await
            .unwrap();
        let snapshot = db.peek().await;
        let catalog = snapshot
            .as_public()
            .as_registry_cache()
            .as_idx(&registry)
            .unwrap()
            .de()
            .unwrap();
        let os = snapshot
            .as_public()
            .as_os_update_cache()
            .de()
            .unwrap()
            .unwrap();
        assert_eq!(catalog.status, RegistryStatus::Connected);
        assert_eq!(os.status, RegistryStatus::Connected);
        assert!(catalog.error.is_none());
        assert!(os.error.is_none());
        eprintln!(
            "{registry}: name={:?}, packages={}, OS versions={}, both connected",
            catalog.info.name,
            catalog.packages.len(),
            os.versions.len()
        );
    }
}
