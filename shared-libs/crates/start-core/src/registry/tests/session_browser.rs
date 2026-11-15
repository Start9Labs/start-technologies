use super::*;

#[tokio::test]
#[ignore = "Requires START_SESSION_BROWSER_PROBE and its built browser fixture"]
async fn real_chromium_signed_dh_transport_and_lifecycle() {
    use axum::extract::Query;
    use axum::routing::post;

    let (_directory, ctx, _, reader, _) = fixture().await;
    let reader_key = reader.verifying_key().await.unwrap();
    let reader_guid = ctx
        .db
        .peek()
        .await
        .as_index()
        .as_signers()
        .get_signer_info(&reader_key)
        .unwrap()
        .0;
    ctx.db
        .mutate(|db| {
            db.as_index_mut()
                .as_name_mut()
                .ser(&Some("x".repeat(17 * 1024 * 1024)))
        })
        .await
        .result
        .unwrap();
    let enrolled = ctx.clone();
    let updated = ctx.clone();
    let app = registry_router(ctx.clone())
        .route(
            "/probe/enroll",
            post(move |axum::Json(key): axum::Json<String>| {
                let ctx = enrolled.clone();
                let guid = reader_guid.clone();
                async move {
                    let key = key.parse::<crate::sign::AnyVerifyingKey>().unwrap();
                    ctx.db
                        .mutate(|db| {
                            let model = db
                                .as_index_mut()
                                .as_signers_mut()
                                .as_idx_mut(&guid)
                                .unwrap();
                            let mut info = model.de()?;
                            info.keys.insert(key);
                            model.ser(&info)
                        })
                        .await
                        .result
                        .unwrap();
                    http::StatusCode::NO_CONTENT
                }
            }),
        )
        .route(
            "/probe/size",
            post(move |Query(query): Query<BTreeMap<String, usize>>| {
                let ctx = updated.clone();
                async move {
                    let name = format!("\u{feff}{}", "x".repeat(query["bytes"]));
                    ctx.db
                        .mutate(|db| db.as_index_mut().as_name_mut().ser(&Some(name)))
                        .await
                        .result
                        .unwrap();
                    http::StatusCode::NO_CONTENT
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let fingerprint = crate::sign::fingerprint::fingerprint(
        &ctx.server_identity()
            .unwrap()
            .verifying_key()
            .await
            .unwrap(),
    );
    let _server: NonDetachingJoinHandle<()> = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    })
    .into();
    let mut child = tokio::process::Command::new("node");
    child
        .arg(std::env::var("START_SESSION_BROWSER_PROBE").unwrap())
        .arg(url)
        .arg(fingerprint)
        .kill_on_drop(true);
    let output = tokio::time::timeout(std::time::Duration::from_secs(120), child.output())
        .await
        .unwrap()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    print!("{}", String::from_utf8_lossy(&output.stdout));
}
