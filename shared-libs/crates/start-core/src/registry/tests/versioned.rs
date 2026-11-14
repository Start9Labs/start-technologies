use axum::extract::ws::Message;
use patch_db::json_ptr::JsonPointer;

use super::session::receive as response;
use super::*;
use crate::registry::os::index::OsVersionInfo;
use crate::util::net::secure::EncryptedSocket;

async fn subscribe(socket: &mut EncryptedSocket, id: u64, version: &str) {
    socket.send(Message::Text(json!({"jsonrpc":"2.0", "rpcstream":"1.0", "id":id, "method":"db.subscribe", "params":{"pointer":"/index", "osVersion":version}}).to_string().into())).await.unwrap();
}

#[tokio::test]
async fn subscribe_selects_each_messages_os_version_for_dumps_and_revisions() {
    let (_temp, ctx, _admin, reader, path) = fixture().await;
    let version: exver::Version = "0.4.0.6".parse().unwrap();
    let mut info = OsVersionInfo {
        release_notes: from_value(json!({"en_US":"English", "pl_PL":"Polski"})).unwrap(),
        ..Default::default()
    };
    ctx.db
        .mutate(|db| {
            db.as_index_mut()
                .as_os_mut()
                .as_versions_mut()
                .insert(&version, &info)
        })
        .await
        .result
        .unwrap();
    let (url, _task) = server(ctx.clone()).await;
    let legacy = rpc(
        &url,
        "db.subscribe",
        json!({"pointer":"/index/os", "osVersion":"0.4.0.4"}),
        None,
    )
    .await;
    assert_eq!(
        legacy["result"]["dump"]["value"]["versions"]["0.4.0.6"]["releaseNotes"],
        json!("English")
    );
    let guid: Guid = from_value(legacy["result"]["guid"].clone()).unwrap();
    drop(ctx.rpc_continuations.get_ws_handler(&guid).await);
    let mut socket = registry_socket(&url, &reader).await;
    subscribe(&mut socket, 71, "0.4.0.2").await;
    subscribe(&mut socket, 72, "0.4.1.3").await;
    let mut views = BTreeMap::new();
    for _ in 0..2 {
        let message = response(&mut socket).await;
        assert!(message.get("error").is_none(), "{message}");
        assert!(message["result"].get("guid").is_none());
        views.insert(
            message["id"].as_u64().unwrap(),
            message["result"]["value"].clone(),
        );
    }
    assert_eq!(
        views[&71]["os"]["versions"]["0.4.0.6"]["releaseNotes"],
        json!("English")
    );
    assert!(views[&72]["os"]["versions"]["0.4.0.6"]["releaseNotes"].is_object());
    assert_eq!(
        views[&71]["package"]["packages"]["secret"]["versions"]["1.0.0:0"]["dependencyMetadata"]["dependency"]
            ["icon"],
        json!("data:image/png;base64,c2VjcmV0")
    );
    assert_eq!(
        views[&72]["package"]["packages"]["secret"]["versions"]["1.0.0:0"]["dependencyMetadata"]["dependency"]
            ["icon"],
        json!(path)
    );
    info.release_notes = from_value(json!({"en_US":"Updated", "pl_PL":"Nowe"})).unwrap();
    ctx.db
        .mutate(|db| {
            db.as_index_mut()
                .as_os_mut()
                .as_versions_mut()
                .insert(&version, &info)
        })
        .await
        .result
        .unwrap();
    for _ in 0..2 {
        let event = response(&mut socket).await;
        assert_eq!(event["rpcstream"], json!("1.0"));
        assert_eq!(event["end"], json!(false));
        let revision: patch_db::Revision = from_value(event["result"].clone()).unwrap();
        patch_db::json_patch::patch(
            views.get_mut(&event["id"].as_u64().unwrap()).unwrap(),
            &revision.patch,
        )
        .unwrap();
    }
    assert_eq!(
        views[&71]["os"]["versions"]["0.4.0.6"]["releaseNotes"],
        json!("Updated")
    );
    assert_eq!(
        views[&72]["os"]["versions"]["0.4.0.6"]["releaseNotes"],
        json!({"en_US":"Updated", "pl_PL":"Nowe"})
    );
    socket.close().await.unwrap();
}

#[tokio::test]
async fn failed_compatibility_replica_ends_only_its_stream() {
    let (temp, ctx, _admin, reader, path) = fixture().await;
    let (url, _task) = server(ctx.clone()).await;
    let mut socket = registry_socket(&url, &reader).await;
    subscribe(&mut socket, 1, "0.4.0.2").await;
    subscribe(&mut socket, 2, "0.4.1.3").await;
    for _ in 0..2 {
        let initial = response(&mut socket).await;
        assert!(initial.get("error").is_none(), "{initial}");
    }
    std::fs::remove_file(icons_dir(temp.path()).join(path)).unwrap();
    ctx.db
        .put(&"/index/name".parse::<JsonPointer>().unwrap(), &"Renamed")
        .await
        .unwrap();
    socket
        .send(Message::Text(
            json!({"jsonrpc":"2.0", "id":3, "method":"index", "params":{}})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let mut seen = BTreeSet::new();
    while seen.len() < 3 {
        let message = response(&mut socket).await;
        let id = message["id"].as_u64().unwrap();
        match id {
            1 => {
                assert_eq!(message["end"], json!(true));
                assert_eq!(
                    message["error"]["code"],
                    json!(ErrorKind::Filesystem as i32)
                );
            }
            2 => {
                assert_eq!(message["end"], json!(false));
                assert!(message.get("error").is_none(), "{message}");
            }
            3 => assert_eq!(message["result"]["name"], json!("Renamed")),
            _ => panic!("unexpected response: {message}"),
        }
        assert!(seen.insert(id));
    }
    socket.close().await.unwrap();
}
