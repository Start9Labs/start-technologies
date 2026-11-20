use super::*;
use crate::sign::commitment::request::RequestProof;

async fn context() -> (tempfile::TempDir, crate::registry::context::RegistryContext) {
    let directory = tempfile::tempdir().unwrap();
    let context = crate::registry::context::test_context(
        directory.path().to_owned(),
        AnySigningKey::generate().verifying_key(),
    )
    .await;
    (directory, context)
}

async fn verified(commitment: RequestCommitment) -> RequestCommitment {
    let signer: Signer = AnySigningKey::generate().into();
    let proof = RequestProof {
        signature: signer
            .sign_commitment(&commitment, "localhost")
            .await
            .unwrap(),
        signer: signer.verifying_key().await.unwrap(),
        commitment,
    };
    proof
        .verify_commitment(b"hello", &["localhost".into()])
        .await
        .unwrap();
    proof.commitment
}

#[tokio::test]
async fn weak_validator_survives_request_scope_and_shares_the_request_nonce_cache() {
    let (_directory, context) = context().await;
    let request = Arc::new(context.clone());
    let check = request.replay_check();
    drop(request);
    let first = verified(RequestCommitment::for_bytes(b"hello")).await;
    check.check(first.clone()).await.unwrap();
    assert!(accept_nonce(&context, &first).await.is_err());

    let second = verified(RequestCommitment::for_bytes(b"hello")).await;
    accept_nonce(&context, &second).await.unwrap();
    assert!(check.check(second).await.is_err());
}

#[tokio::test]
async fn weak_validator_does_not_retain_the_authentication_owner() {
    let (_directory, context) = context().await;
    let check = context.replay_check();
    drop(context);
    let commitment = verified(RequestCommitment::for_bytes(b"hello")).await;
    assert!(check.check(commitment).await.is_err());
}

#[tokio::test]
async fn an_unpolled_check_does_not_retain_or_revive_the_authentication_owner() {
    let (_directory, context) = context().await;
    let check = context.replay_check();
    let commitment = verified(RequestCommitment::for_bytes(b"hello")).await;
    let pending = check.check(commitment);
    drop(context);
    assert!(pending.await.is_err());
}

#[tokio::test]
async fn validator_uses_the_existing_clock_policy_without_consuming_rejected_nonces() {
    let (_directory, context) = context().await;
    let check = context.replay_check();
    let mut commitment = RequestCommitment::for_bytes(b"hello");
    let now = commitment.timestamp;
    commitment.timestamp = 0;
    let expired = verified(commitment.clone()).await;
    assert!(check.check(expired).await.is_err());
    commitment.timestamp = now;
    check.check(verified(commitment).await).await.unwrap();
}
