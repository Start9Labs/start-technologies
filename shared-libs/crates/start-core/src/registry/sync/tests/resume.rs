use super::live::{encrypted_server, signer};
use super::*;
use crate::sign::session::{PreparedClient, Target};
use crate::util::net::secure::EncryptedSocket;
use crate::util::net::tungstenite::transport;

#[tokio::test]
async fn oversized_replay_retries_from_a_dump_without_discarding_cached_data() {
    let limit = std::num::NonZeroUsize::new(8192).unwrap();
    let source = PatchDb::in_memory();
    source.put(&ROOT, &index()).await.unwrap();
    let (epoch, catchup, _) = source.catch_up_and_sub(ROOT.to_owned(), None).await;
    let patch_db::Catchup::Dump(initial) = catchup else {
        panic!("expected a dump")
    };
    for name in ["a", "b", "c"] {
        source
            .put(
                &"/name".parse::<patch_db::json_ptr::JsonPointer>().unwrap(),
                &name.repeat(4096),
            )
            .await
            .unwrap();
    }
    let (_, catchup, _) = source
        .catch_up_and_sub(ROOT.to_owned(), Some((&epoch, initial.id)))
        .await;
    let patch_db::Catchup::Replay { id, revisions } = catchup else {
        panic!("expected a replay")
    };
    let current = source.dump(&ROOT).await;
    let responses = [
        json!({"jsonrpc":"2.0", "id":1, "result":{
            "epoch":epoch, "id":id, "value":null, "revisions":revisions,
        }})
        .to_string(),
        json!({"jsonrpc":"2.0", "id":1, "result":{
            "epoch":epoch, "id":current.id, "value":current.value, "revisions":null,
        }})
        .to_string(),
    ];
    assert!(responses[0].len() > limit.get());
    assert!(responses[1].len() < limit.get());

    let db = TypedPatchDb::<Database>::load_unchecked(PatchDb::in_memory());
    db.put(&ROOT, database().as_value()).await.unwrap();
    let view = RegistryView::new(&device());
    db.mutate(|db| {
        ingest_event(
            db,
            &url(),
            &view,
            &connection::SubscriptionEvent::Start(connection::SubscriptionStart {
                epoch: Some(epoch.clone()),
                id: initial.id,
                value: initial.value.clone(),
                revisions: None,
            }),
            &device(),
        )
    })
    .await
    .result
    .unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let expected = json!({"epoch":epoch, "id":initial.id});
    let server: NonDetachingJoinHandle<()> = tokio::spawn(async move {
        for (attempt, response) in responses.into_iter().enumerate() {
            let (stream, _) = listener.accept().await.unwrap();
            let socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            let mut socket = encrypted_server(socket).await;
            let Message::Text(request) = socket.next().await.unwrap().unwrap() else {
                panic!("expected a subscription request")
            };
            let request: Value = serde_json::from_str(&request).unwrap();
            if attempt == 0 {
                assert_eq!(request["params"]["since"], expected);
            } else {
                assert!(request["params"].get("since").is_none());
            }
            socket.send(Message::Text(response.into())).await.unwrap();
            if attempt == 1 {
                socket.send(Message::Ping(vec![1].into())).await.unwrap();
                assert_eq!(
                    socket.next().await.unwrap().unwrap(),
                    Message::Pong(vec![1].into())
                );
                socket.send(Message::Text("{".into())).await.unwrap();
            }
        }
    })
    .into();

    for attempt in 0..2 {
        let (socket, _) = tokio_tungstenite::connect_async(format!("ws://{address}"))
            .await
            .unwrap();
        let prepared = PreparedClient::new(&signer(), "registry.test", Target::Rpc)
            .await
            .unwrap();
        let (_, socket) = EncryptedSocket::initiate(transport(socket), prepared, limit)
            .await
            .unwrap();
        let mut backoff = 300;
        assert!(
            tokio::time::timeout(
                Duration::from_secs(5),
                receive_updates(
                    socket,
                    &db,
                    &BTreeSet::from([Subscription::Index(url())]),
                    &device(),
                    &mut backoff,
                ),
            )
            .await
            .unwrap()
            .is_err()
        );
        let saved = db.peek().await;
        let cursor = resumable_cursor(&saved, &url(), &view).unwrap();
        let expected = if attempt == 0 {
            assert!(cursor.is_none());
            &initial.value
        } else {
            assert_eq!(cursor.unwrap().id, current.id);
            assert_eq!(backoff, INITIAL_BACKOFF);
            &current.value
        };
        assert_eq!(
            saved
                .as_public()
                .as_registry_indexes()
                .as_idx(&url())
                .unwrap()
                .as_value(),
            expected
        );
        assert_eq!(
            saved
                .as_public()
                .as_registry_cache()
                .as_idx(&url())
                .unwrap()
                .de()
                .unwrap()
                .info
                .name
                .as_deref(),
            expected["name"].as_str()
        );
    }
    server.await.unwrap();
}
