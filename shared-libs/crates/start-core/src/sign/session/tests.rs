use std::sync::atomic::{AtomicBool, Ordering};

use super::*;
use crate::sign::AnySigningKey;

fn signer(seed: u8) -> Signer {
    AnySigningKey::Ed25519(ed25519_dalek::SigningKey::from_bytes(&[seed; 32])).into()
}

async fn resign(
    wire: &[u8],
    signer: &Signer,
    context: &str,
    change: impl FnOnce(&mut Message),
) -> Vec<u8> {
    let mut envelope: Envelope = serde_json::from_slice(wire).unwrap();
    let mut message: Message = serde_json::from_slice(&envelope.payload).unwrap();
    change(&mut message);
    envelope.payload = Base64(serde_json::to_vec(&message).unwrap());
    envelope.proof = RequestProof::sign_commitment(signer, &envelope.payload, context)
        .await
        .unwrap()
        .to_header()
        .to_str()
        .unwrap()
        .into();
    serde_json::to_vec(&envelope).unwrap()
}

async fn pair() -> (SessionCipher, SessionCipher) {
    let client = PreparedClient::new(&signer(1), "server.test")
        .await
        .unwrap();
    let (_, response, server) = accept(
        &signer(2),
        "server.test",
        client.message(),
        None,
        |_| async { Ok(()) },
    )
    .await
    .unwrap();
    let (_, client) = client.finish(&response).await.unwrap();
    (client, server)
}

fn encrypted(cipher: &mut SessionCipher, plaintext: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0; plaintext.len() + TAG_BYTES];
    let length = cipher.write_message(plaintext, &mut bytes).unwrap();
    bytes.truncate(length);
    bytes
}

#[tokio::test]
async fn two_messages_authenticate_both_identities_and_both_record_directions() {
    let client_key = signer(1);
    let server_key = signer(2);
    let client = PreparedClient::new(&client_key, "server.test")
        .await
        .unwrap();
    let called = AtomicBool::new(false);
    let (peer, response, mut server) = accept(
        &server_key,
        "server.test",
        client.message(),
        None,
        |_| async {
            called.store(true, Ordering::SeqCst);
            Ok(())
        },
    )
    .await
    .unwrap();
    assert!(called.load(Ordering::SeqCst));
    assert_eq!(peer, client_key.verifying_key().await.unwrap());
    assert!(client.message().len() <= MAX_HANDSHAKE_BYTES);
    assert!(response.len() <= MAX_HANDSHAKE_BYTES);
    let (peer, mut client) = client.finish(&response).await.unwrap();
    assert_eq!(peer, server_key.verifying_key().await.unwrap());
    let mut plain = [0; 64];
    let message = encrypted(&mut client, b"client RPC");
    let n = server.read_message(&message, &mut plain).unwrap();
    assert_eq!(&plain[..n], b"client RPC");
    let message = encrypted(&mut server, b"server response");
    let n = client.read_message(&message, &mut plain).unwrap();
    assert_eq!(&plain[..n], b"server response");
}

#[tokio::test]
async fn pure_ed25519_client_and_ed25519ph_server_use_the_shared_proof_abstractions() {
    let client_key = signer(3);
    let mut client = PreparedClient::new(&client_key, "server.test")
        .await
        .unwrap();
    let mut envelope: Envelope = serde_json::from_slice(client.message()).unwrap();
    let mut proof =
        RequestProof::from_header(&http::HeaderValue::from_str(&envelope.proof).unwrap()).unwrap();
    proof.signature = client_key
        .sign_message(
            &crate::sign::signing_message(
                crate::sign::COMMITMENT_SIG_TAG,
                &proof.commitment,
                "server.test",
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert!(matches!(
        proof.signature,
        crate::sign::AnySignature::Ed25519(_)
    ));
    envelope.proof = proof.to_header().to_str().unwrap().into();
    client.message = serde_json::to_vec(&envelope).unwrap();
    let (_, response, mut server) = accept(
        &signer(4),
        "server.test",
        client.message(),
        None,
        |_| async { Ok(()) },
    )
    .await
    .unwrap();
    let (_, proof) = decode(&response, "server.test").await.unwrap();
    assert!(matches!(
        proof.signature,
        crate::sign::AnySignature::Ed25519ph(_)
    ));
    let (_, mut client) = client.finish(&response).await.unwrap();
    let mut plaintext = [0; 64];
    assert_eq!(
        server
            .read_message(
                &encrypted(&mut client, b"independent schemes"),
                &mut plaintext
            )
            .unwrap(),
        19
    );
}

#[tokio::test]
async fn initiation_is_fresh_for_each_connection() {
    let first = PreparedClient::new(&signer(5), "server.test")
        .await
        .unwrap();
    let second = PreparedClient::new(&signer(5), "server.test")
        .await
        .unwrap();
    assert_ne!(first.message(), second.message());
    let (_, a) = decode(first.message(), "server.test").await.unwrap();
    let (_, b) = decode(second.message(), "server.test").await.unwrap();
    assert_ne!(a.commitment.nonce, b.commitment.nonce);
}

#[tokio::test]
async fn full_width_nonce_reaches_the_existing_replay_policy_without_json_rounding() {
    let key = signer(6);
    let mut client = PreparedClient::new(&key, "server.test").await.unwrap();
    let mut envelope: Envelope = serde_json::from_slice(client.message()).unwrap();
    let mut proof =
        RequestProof::from_header(&http::HeaderValue::from_str(&envelope.proof).unwrap()).unwrap();
    proof.commitment.nonce = u64::MAX;
    proof.signature = key
        .sign_commitment(&proof.commitment, "server.test")
        .await
        .unwrap();
    envelope.proof = proof.to_header().to_str().unwrap().into();
    client.message = serde_json::to_vec(&envelope).unwrap();
    let (_, response, _) = accept(
        &signer(7),
        "server.test",
        client.message(),
        None,
        |commitment| async move {
            assert_eq!(commitment.nonce, u64::MAX);
            Ok(())
        },
    )
    .await
    .unwrap();
    client.finish(&response).await.unwrap();
}

#[tokio::test]
async fn server_response_cannot_be_replayed_into_another_connection() {
    let first = PreparedClient::new(&signer(8), "server.test")
        .await
        .unwrap();
    let second = PreparedClient::new(&signer(8), "server.test")
        .await
        .unwrap();
    let (_, response, _) = accept(
        &signer(9),
        "server.test",
        first.message(),
        None,
        |_| async { Ok(()) },
    )
    .await
    .unwrap();
    assert!(second.finish(&response).await.is_err());
    first.finish(&response).await.unwrap();
}

#[tokio::test]
async fn context_role_and_protocol_are_authenticated() {
    let key = signer(10);
    let client = PreparedClient::new(&key, "server.test").await.unwrap();
    assert!(
        accept(
            &signer(11),
            "other.test",
            client.message(),
            None,
            |_| async { panic!("invalid proof reached nonce policy") }
        )
        .await
        .is_err()
    );
    for changed in [
        resign(client.message(), &key, "server.test", |message| {
            message.version += 1
        })
        .await,
        resign(client.message(), &key, "server.test", |message| {
            message.protocol = "other protocol".into()
        })
        .await,
        resign(client.message(), &key, "server.test", |message| {
            message.hello = Hello::Server {
                ephemeral: Base64([1; 32]),
                client_hello: Base64([0; 32]),
            }
        })
        .await,
    ] {
        assert!(
            accept(&signer(11), "server.test", &changed, None, |_| async {
                panic!("invalid hello reached nonce policy")
            })
            .await
            .is_err()
        );
    }
}

#[tokio::test]
async fn expected_peer_and_contributory_dh_are_checked_before_nonce_policy() {
    let key = signer(12);
    let client = PreparedClient::new(&key, "server.test").await.unwrap();
    assert!(
        accept(
            &signer(13),
            "server.test",
            client.message(),
            Some(&signer(14).verifying_key().await.unwrap()),
            |_| async { panic!("wrong peer reached nonce policy") }
        )
        .await
        .is_err()
    );
    let low_order = resign(client.message(), &key, "server.test", |message| {
        message.hello = Hello::Client {
            ephemeral: Base64([0; 32]),
        };
    })
    .await;
    assert!(
        accept(&signer(13), "server.test", &low_order, None, |_| async {
            panic!("noncontributory DH reached nonce policy")
        })
        .await
        .is_err()
    );
}

#[tokio::test]
async fn replay_policy_rejection_cannot_produce_a_session() {
    let client = PreparedClient::new(&signer(15), "server.test")
        .await
        .unwrap();
    assert!(
        accept(
            &signer(16),
            "server.test",
            client.message(),
            None,
            |_| async { Err(invalid()) }
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn body_and_identity_substitution_fail_proof_verification() {
    let key = signer(17);
    let client = PreparedClient::new(&key, "server.test").await.unwrap();
    let mut envelope: Envelope = serde_json::from_slice(client.message()).unwrap();
    envelope.payload.0.push(b' ');
    let changed = serde_json::to_vec(&envelope).unwrap();
    assert!(
        accept(&signer(18), "server.test", &changed, None, |_| async {
            panic!("changed payload reached nonce policy")
        })
        .await
        .is_err()
    );
    let mut envelope: Envelope = serde_json::from_slice(client.message()).unwrap();
    let mut proof =
        RequestProof::from_header(&http::HeaderValue::from_str(&envelope.proof).unwrap()).unwrap();
    proof.signer = signer(19).verifying_key().await.unwrap();
    envelope.proof = proof.to_header().to_str().unwrap().into();
    let changed = serde_json::to_vec(&envelope).unwrap();
    assert!(
        accept(&signer(18), "server.test", &changed, None, |_| async {
            panic!("substituted identity reached nonce policy")
        })
        .await
        .is_err()
    );
}

#[tokio::test]
async fn record_authentication_rejects_tampering_replay_reflection_and_reordering() {
    let (mut client, mut server) = pair().await;
    let _first = encrypted(&mut client, b"first");
    let second = encrypted(&mut client, b"second");
    let mut plaintext = [0; 64];
    assert!(server.read_message(&second, &mut plaintext).is_err());

    let (mut client, _) = pair().await;
    let first = encrypted(&mut client, b"first");
    assert!(client.read_message(&first, &mut plaintext).is_err());

    let (mut client, mut server) = pair().await;
    let mut tampered = encrypted(&mut client, b"first");
    tampered[0] ^= 1;
    assert!(server.read_message(&tampered, &mut plaintext).is_err());

    let (mut client, mut server) = pair().await;
    let first = encrypted(&mut client, b"first");
    assert_eq!(server.read_message(&first, &mut plaintext).unwrap(), 5);
    assert!(server.read_message(&first, &mut plaintext).is_err());
}

#[tokio::test]
async fn counter_exhaustion_fails_before_nonce_reuse_and_short_buffers_are_rejected() {
    let (mut client, mut server) = pair().await;
    assert!(client.write_message(&[], &mut []).is_err());
    client.send.counter = MAX_RECORDS_PER_KEY - 1;
    server.receive.counter = MAX_RECORDS_PER_KEY - 1;
    let record = encrypted(&mut client, b"last");
    let mut plaintext = [0; 32];
    assert_eq!(server.read_message(&record, &mut plaintext).unwrap(), 4);
    assert!(client.write_message(b"more", &mut [0; 32]).is_err());
    assert!(server.read_message(&record, &mut plaintext).is_err());
    assert_eq!(client.send.counter, MAX_RECORDS_PER_KEY);
    assert_eq!(server.receive.counter, MAX_RECORDS_PER_KEY);
    let (_, mut server) = pair().await;
    assert!(server.read_message(&[], &mut []).is_err());
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebCryptoFixture {
    context: String,
    client_private: String,
    client_identity: AnyVerifyingKey,
    server_identity: AnyVerifyingKey,
    client: Base64<Vec<u8>>,
    server: Base64<Vec<u8>>,
    client_record: WebCryptoRecord,
    server_record: WebCryptoRecord,
}

#[derive(Deserialize)]
struct WebCryptoRecord {
    plaintext: String,
    ciphertext: Base64<Vec<u8>>,
}

#[tokio::test]
async fn independent_webcrypto_proofs_dh_hkdf_and_aes_gcm_match_in_both_directions() {
    let fixture: WebCryptoFixture = serde_json::from_str(include_str!(
        "../../../testdata/signed-session/webcrypto.json"
    ))
    .unwrap();
    let client = PreparedClient {
        secret: StaticSecret::from(
            <[u8; 32]>::try_from(hex::decode(&fixture.client_private).unwrap()).unwrap(),
        ),
        context: fixture.context,
        message: fixture.client.0,
    };
    let (peer, mut cipher) = client.finish(&fixture.server).await.unwrap();
    assert_eq!(peer, fixture.server_identity);
    assert_eq!(
        encrypted(&mut cipher, fixture.client_record.plaintext.as_bytes()),
        fixture.client_record.ciphertext.0
    );
    let mut plaintext = [0; 128];
    let n = cipher
        .read_message(&fixture.server_record.ciphertext, &mut plaintext)
        .unwrap();
    assert_eq!(&plaintext[..n], fixture.server_record.plaintext.as_bytes());
}

#[tokio::test]
async fn native_server_accepts_webcrypto_identity_and_request_commitment() {
    let fixture: WebCryptoFixture = serde_json::from_str(include_str!(
        "../../../testdata/signed-session/webcrypto.json"
    ))
    .unwrap();
    let (peer, response, mut server) = accept(
        &signer(21),
        &fixture.context,
        &fixture.client,
        None,
        |proof| async move {
            assert_eq!(proof.nonce, u64::MAX);
            Ok(())
        },
    )
    .await
    .unwrap();
    assert_eq!(peer, fixture.client_identity);
    let client = PreparedClient {
        secret: StaticSecret::from(
            <[u8; 32]>::try_from(hex::decode(&fixture.client_private).unwrap()).unwrap(),
        ),
        context: fixture.context,
        message: fixture.client.0,
    };
    let (_, mut client) = client.finish(&response).await.unwrap();
    let mut plaintext = [0; 128];
    let ciphertext = encrypted(&mut client, b"WebCrypto initiation with native server");
    let n = server.read_message(&ciphertext, &mut plaintext).unwrap();
    assert_eq!(&plaintext[..n], b"WebCrypto initiation with native server");
}

#[tokio::test]
async fn malformed_and_oversized_envelopes_do_not_reach_replay_policy() {
    for wire in [
        vec![0; MAX_HANDSHAKE_BYTES + 1],
        b"{\"payload\":\"bad\",\"proof\":\"\"}".to_vec(),
        b"{}".to_vec(),
    ] {
        assert!(
            accept(&signer(20), "server.test", &wire, None, |_| async {
                panic!("invalid envelope reached nonce policy")
            })
            .await
            .is_err()
        );
    }
    assert_eq!(wire_profile()["version"], VERSION);
}
