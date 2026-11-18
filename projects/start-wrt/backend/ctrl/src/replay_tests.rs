use startos::sign::commitment::request::{RequestCommitment, RequestProof};

use super::*;

async fn commitment() -> RequestCommitment {
    let signer = startos::sign::AnySigningKey::generate().into();
    let proof = RequestProof::sign_commitment(&signer, b"hello", "router.lan")
        .await
        .unwrap();
    proof
        .verify_commitment(b"hello", &["router.lan".into()])
        .await
        .unwrap();
    proof.commitment
}

#[tokio::test]
async fn replay_validator_retains_neither_authentication_nor_continuation_owners() {
    let context = ServerContext::default();
    let check = context.replay_check();
    assert_eq!(Arc::strong_count(&context.auth), 1);
    assert_eq!(Arc::strong_count(&context.continuations), 1);
    let _authentication = context.auth.clone();
    drop(context);
    assert!(check.check(commitment().await).await.is_err());
}

#[tokio::test]
async fn replay_validator_survives_request_clones_but_not_server_context_lifetime() {
    let context = ServerContext::default();
    let check = context.replay_check();
    let active = context.clone();
    drop(context);
    check.check(commitment().await).await.unwrap();
    drop(active);
    assert!(check.check(commitment().await).await.is_err());
}
