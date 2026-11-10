use super::*;
use crate::rpc_continuations::Guid;

pub(super) async fn context(datadir: PathBuf, uploader: AnyVerifyingKey) -> RegistryContext {
    tokio::fs::create_dir_all(&datadir).await.unwrap();
    let db =
        TypedPatchDb::load_unchecked(PatchDb::open(datadir.join("registry.db")).await.unwrap());
    let mut database = RegistryDatabase::init();
    let guid = Guid::new();
    database.admins.insert(guid.clone());
    database.index.trust_signers.insert(guid.clone());
    database.index.signers.insert(
        guid,
        SignerInfo {
            name: "Uploader".into(),
            contact: vec![],
            keys: [uploader].into(),
            gpg_exports: Default::default(),
        },
    );
    db.put(&ROOT, &database).await.unwrap();
    RegistryContext(Arc::new(RegistryContextSeed {
        hostnames: vec!["localhost".into(), "127.0.0.1".into()],
        listen: "127.0.0.1:0".parse().unwrap(),
        db,
        datadir,
        product: Product::StartOs,
        rpc_continuations: RpcContinuations::new(),
        open_authed_continuations: OpenAuthedContinuations::new(),
        auth_sig_nonce_cache: SyncMutex::new(Default::default()),
        client: Client::new(),
        shutdown: tokio::sync::broadcast::channel(1).0,
        metrics_db: SyncMutex::new(Connection::open_in_memory().unwrap()),
        security_feed: None,
    }))
}

pub(super) async fn registry_server(
    ctx: RegistryContext,
) -> (Url, crate::util::future::NonDetachingJoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, crate::registry::registry_router(ctx))
            .await
            .unwrap();
    });
    (url, task.into())
}
