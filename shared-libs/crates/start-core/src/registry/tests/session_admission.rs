use super::*;
use crate::sign::commitment::request::RequestProof;
use crate::sign::commitment::request::binary::BinaryRequestProof;

async fn rejected(url: &str, wire: &[u8]) {
    let mut raw = raw_socket(url).await;
    let mut framed = (wire.len() as u16).to_be_bytes().to_vec();
    framed.extend_from_slice(wire);
    raw.send(LogicalMessage::Binary(framed.into()))
        .await
        .unwrap();
    assert!(!matches!(
        tokio::time::timeout(Duration::from_secs(3), raw.next())
            .await
            .unwrap(),
        Some(Ok(LogicalMessage::Binary(_)))
    ));
}

fn proof(wire: &[u8]) -> (&[u8], RequestProof) {
    let mut reader = std::io::Cursor::new(wire);
    let _: serde_cbor::Value = serde_cbor::de::from_reader(&mut reader).unwrap();
    let end = reader.position() as usize;
    let proof: BinaryRequestProof = serde_cbor::de::from_reader(&mut reader).unwrap();
    (&wire[..end], proof.into())
}

#[tokio::test]
async fn replayed_rpc_initiation_is_rejected_by_the_live_admission_policy() {
    let (_temp, ctx, _admin, reader, _) = fixture().await;
    let (url, _task) = session_server(ctx).await;
    let prepared = PreparedClient::new(&reader, "127.0.0.1", Target::Rpc)
        .await
        .unwrap();
    let captured = prepared.message().to_vec();
    let (_peer, _socket) = EncryptedSocket::initiate(
        raw_socket(&secure_url(&url)).await,
        prepared,
        NonZeroUsize::new(1024).unwrap(),
    )
    .await
    .unwrap();
    rejected(&secure_url(&url), &captured).await;
}

#[tokio::test]
async fn rpc_intent_cannot_redeem_a_guid_or_burn_the_rpc_nonce() {
    let (_temp, ctx, _admin, reader, _) = fixture().await;
    let (url, _task) = session_server(ctx).await;
    let mut socket = encrypted(&secure_url(&url), &reader).await;
    let guid = subscribe(&mut socket).await;
    let prepared = PreparedClient::new(&reader, "127.0.0.1", Target::Rpc)
        .await
        .unwrap();
    rejected(&ws_url(&url, &format!("ws/rpc/{guid}")), prepared.message()).await;
    EncryptedSocket::initiate(
        raw_socket(&secure_url(&url)).await,
        prepared,
        NonZeroUsize::new(1024).unwrap(),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn continuation_intent_rejects_a_different_guid_before_nonce_consumption() {
    let (_temp, ctx, _admin, reader, _) = fixture().await;
    let (url, _task) = session_server(ctx.clone()).await;
    let mut socket = encrypted(&secure_url(&url), &reader).await;
    let first = subscribe(&mut socket).await;
    let other = subscribe(&mut socket).await;
    let prepared = PreparedClient::new(
        &reader,
        "127.0.0.1",
        Target::Continuation(first.parse().unwrap()),
    )
    .await
    .unwrap();
    rejected(
        &ws_url(&url, &format!("ws/rpc/{other}")),
        prepared.message(),
    )
    .await;
    let (_, mut stream) = EncryptedSocket::initiate(
        raw_socket(&ws_url(&url, &format!("ws/rpc/{first}"))).await,
        prepared,
        NonZeroUsize::new(64 * 1024 * 1024).unwrap(),
    )
    .await
    .unwrap();
    revoke_private_reader(&ctx).await;
    assert!(receive(&mut stream).await.get("patch").is_some());
}

#[tokio::test]
async fn rpc_and_guid_admission_consume_the_same_nonce_cache() {
    let (_temp, ctx, _admin, reader, _) = fixture().await;
    let (url, _task) = session_server(ctx).await;
    let prepared = PreparedClient::new(&reader, "127.0.0.1", Target::Rpc)
        .await
        .unwrap();
    let nonce = proof(prepared.message()).1.commitment.nonce;
    let (_, mut socket) = EncryptedSocket::initiate(
        raw_socket(&secure_url(&url)).await,
        prepared,
        NonZeroUsize::new(64 * 1024 * 1024).unwrap(),
    )
    .await
    .unwrap();
    let guid = subscribe(&mut socket).await;
    let prepared = PreparedClient::new(
        &reader,
        "127.0.0.1",
        Target::Continuation(guid.parse().unwrap()),
    )
    .await
    .unwrap();
    let (body, mut header) = proof(prepared.message());
    header.commitment.nonce = nonce;
    header.signature = reader
        .sign_commitment(&header.commitment, "127.0.0.1")
        .await
        .unwrap();
    let mut wire = body.to_vec();
    serde_cbor::ser::into_writer(&BinaryRequestProof::from(&header), &mut wire).unwrap();
    rejected(&ws_url(&url, &format!("ws/rpc/{guid}")), &wire).await;
}
