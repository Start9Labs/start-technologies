use std::sync::atomic::{AtomicBool, Ordering};

use super::*;
use crate::sign::AnySigningKey;
use crate::util::serde::{Base64, DerEncoding};

fn signer(seed: u8) -> Signer {
    AnySigningKey::Ed25519(ed25519_dalek::SigningKey::from_bytes(&[seed; 32])).into()
}

async fn client(key: &Signer) -> PreparedClient {
    PreparedClient::new(key, "server.test", Target::Rpc)
        .await
        .unwrap()
}

async fn accept_rpc(key: &Signer, wire: &[u8]) -> (AnyVerifyingKey, Vec<u8>, SessionCipher) {
    let (peer, context, response, cipher) = accept(
        key,
        &["server.test".into()],
        wire,
        &Target::Rpc,
        None,
        |_| async { Ok(()) },
    )
    .await
    .unwrap();
    assert_eq!(&*context, "server.test");
    (peer, response, cipher)
}

async fn reject_before_nonce(wire: &[u8], context: &str, peer: Option<&AnyVerifyingKey>) {
    assert!(
        accept(
            &signer(2),
            &[context.into()],
            wire,
            &Target::Rpc,
            peer,
            |_| async { panic!("invalid initiation reached nonce policy") }
        )
        .await
        .is_err()
    );
}

fn split_proof(wire: &[u8]) -> (&[u8], RequestProof) {
    let mut reader = std::io::Cursor::new(wire);
    let _: Message = serde_cbor::de::from_reader(&mut reader).unwrap();
    let end = reader.position() as usize;
    let proof: BinaryRequestProof = serde_cbor::de::from_reader(&mut reader).unwrap();
    assert_eq!(reader.position() as usize, wire.len());
    (&wire[..end], proof.into())
}

fn with_proof(body: &[u8], proof: &RequestProof) -> Vec<u8> {
    let mut wire = body.to_vec();
    serde_cbor::ser::into_writer(&BinaryRequestProof::from(proof), &mut wire).unwrap();
    wire
}

async fn resign(wire: &[u8], key: &Signer, change: impl FnOnce(&mut Message)) -> Vec<u8> {
    let (body, _) = split_proof(wire);
    let mut message: Message = serde_cbor::de::from_reader(body).unwrap();
    change(&mut message);
    let mut body = Vec::new();
    serde_cbor::ser::into_writer(&message, &mut body).unwrap();
    let proof = RequestProof::sign_commitment(key, &body, "server.test")
        .await
        .unwrap();
    with_proof(&body, &proof)
}

fn cbor_field<'a>(value: &'a mut serde_cbor::Value, name: &str) -> &'a mut serde_cbor::Value {
    let serde_cbor::Value::Map(fields) = value else {
        panic!("expected map")
    };
    &mut fields
        .iter_mut()
        .find(|(key, _)| key == &serde_cbor::Value::Text(name.into()))
        .unwrap()
        .1
}

#[tokio::test]
async fn strict_admission_kinds_reject_signed_alternative_representations() {
    use serde_cbor::Value;

    let key = signer(1);
    let original = client(&key).await;
    let (original_body, _) = split_proof(original.message());
    for case in 0..6 {
        let mut body: Value = serde_cbor::de::from_reader(original_body).unwrap();
        match case {
            0 => {
                let value = cbor_field(&mut body, "protocol");
                *value = Value::Tag(999, Box::new(value.clone()));
            }
            1 => {
                let Value::Map(fields) = &mut body else {
                    unreachable!()
                };
                fields[0].0 = Value::Bytes(b"protocol".to_vec());
            }
            2 => {
                let Value::Map(fields) = cbor_field(&mut body, "hello") else {
                    unreachable!()
                };
                for (name, _) in fields {
                    if name == &Value::Text("ephemeral".into()) {
                        *name = Value::Integer(0.into());
                    } else if name == &Value::Text("target".into()) {
                        *name = Value::Integer(1.into());
                    }
                }
            }
            3 => {
                let hello = cbor_field(&mut body, "hello");
                let ephemeral = cbor_field(hello, "ephemeral").clone();
                *hello = Value::Array(vec![
                    Value::Text("client".into()),
                    ephemeral,
                    Value::Text("rpc".into()),
                ]);
            }
            4 => {
                let value = cbor_field(cbor_field(&mut body, "hello"), "ephemeral");
                *value = Value::Tag(64, Box::new(value.clone()));
            }
            5 => {}
            _ => unreachable!(),
        }
        let mut wire_body = Vec::new();
        serde_cbor::ser::into_writer(&body, &mut wire_body).unwrap();
        if case == 5 {
            let offset = wire_body
                .windows(2)
                .position(|bytes| bytes == [0x58, 0x20])
                .unwrap();
            wire_body.insert(offset, 0x5f);
            wire_body.insert(offset + 35, 0xff);
        }
        let proof = RequestProof::sign_commitment(&key, &wire_body, "server.test")
            .await
            .unwrap();
        assert!(
            accept(
                &signer(2),
                &["server.test".into()],
                &with_proof(&wire_body, &proof),
                &Target::Rpc,
                None,
                |_| async { Ok(()) }
            )
            .await
            .is_err(),
            "alternative representation {case} was admitted"
        );
    }
}

#[tokio::test]
async fn strict_admission_kinds_also_apply_to_proof_metadata() {
    use serde_cbor::Value;

    let key = signer(1);
    let original = client(&key).await;
    let (body, proof) = split_proof(original.message());
    let mut encoded = Vec::new();
    serde_cbor::ser::into_writer(&BinaryRequestProof::from(&proof), &mut encoded).unwrap();
    for case in 0..3 {
        let mut value: Value = serde_cbor::de::from_reader(encoded.as_slice()).unwrap();
        match case {
            0 => {
                let signature = cbor_field(&mut value, "signature");
                *signature = Value::Tag(999, Box::new(signature.clone()));
            }
            1 => {
                let nonce = cbor_field(cbor_field(&mut value, "commitment"), "nonce");
                *nonce = Value::Tag(999, Box::new(nonce.clone()));
            }
            2 => {
                let Value::Map(fields) = cbor_field(&mut value, "commitment") else {
                    unreachable!()
                };
                fields[0].0 = Value::Bytes(b"timestamp".to_vec());
            }
            _ => unreachable!(),
        }
        let mut wire = body.to_vec();
        serde_cbor::ser::into_writer(&value, &mut wire).unwrap();
        reject_before_nonce(&wire, "server.test", None).await;
    }
}

async fn pair() -> (SessionCipher, SessionCipher) {
    let client = client(&signer(1)).await;
    let (_, response, server) = accept_rpc(&signer(2), client.message()).await;
    (client.finish(&response).await.unwrap().1, server)
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
    let client = client(&client_key).await;
    let called = AtomicBool::new(false);
    let (peer, context, response, mut server) = accept(
        &server_key,
        &["other.test".into(), "server.test".into()],
        client.message(),
        &Target::Rpc,
        None,
        |_| async {
            called.store(true, Ordering::SeqCst);
            Ok(())
        },
    )
    .await
    .unwrap();
    assert!(called.load(Ordering::SeqCst));
    assert_eq!(&*context, "server.test");
    assert_eq!(peer, client_key.verifying_key().await.unwrap());
    assert!(client.message().len() <= MAX_HANDSHAKE_BYTES);
    assert!(response.len() <= MAX_HANDSHAKE_BYTES);
    let (peer, mut client) = client.finish(&response).await.unwrap();
    assert_eq!(peer, server_key.verifying_key().await.unwrap());
    let mut plain = [0; 64];
    let n = server
        .read_message(&encrypted(&mut client, b"client RPC"), &mut plain)
        .unwrap();
    assert_eq!(&plain[..n], b"client RPC");
    let n = client
        .read_message(&encrypted(&mut server, b"server response"), &mut plain)
        .unwrap();
    assert_eq!(&plain[..n], b"server response");
}

#[tokio::test]
async fn missing_or_unconfigured_ingress_identities_fail_before_nonce_consumption() {
    let client = client(&signer(25)).await;
    for contexts in [vec![], vec!["other.test".into()]] {
        assert!(
            accept(
                &signer(26),
                &contexts,
                client.message(),
                &Target::Rpc,
                None,
                |_| async { panic!("unconfigured context reached nonce policy") }
            )
            .await
            .is_err()
        );
    }
}

#[tokio::test]
async fn generated_client_and_server_handshake_proofs_use_pure_ed25519() {
    let client = client(&signer(3)).await;
    assert!(matches!(
        split_proof(client.message()).1.signature,
        crate::sign::AnySignature::Ed25519(_)
    ));
    let (_, response, mut server) = accept_rpc(&signer(4), client.message()).await;
    assert!(matches!(
        split_proof(&response).1.signature,
        crate::sign::AnySignature::Ed25519(_)
    ));
    let (_, mut client) = client.finish(&response).await.unwrap();
    let mut plain = [0; 64];
    let n = server
        .read_message(&encrypted(&mut client, b"pure Ed25519"), &mut plain)
        .unwrap();
    assert_eq!(&plain[..n], b"pure Ed25519");
}

#[tokio::test]
async fn compatibility_ed25519ph_proofs_remain_accepted_in_both_roles() {
    let client_key = signer(3);
    let server_key = signer(4);
    let mut client = client(&client_key).await;
    let (body, mut proof) = split_proof(client.message());
    proof.signature = client_key
        .sign_commitment(&proof.commitment, "server.test")
        .await
        .unwrap();
    assert!(matches!(
        proof.signature,
        crate::sign::AnySignature::Ed25519ph(_)
    ));
    client.message = with_proof(body, &proof);
    let (_, response, _) = accept_rpc(&server_key, client.message()).await;
    let (body, mut proof) = split_proof(&response);
    proof.signature = server_key
        .sign_commitment(&proof.commitment, "server.test")
        .await
        .unwrap();
    assert!(matches!(
        proof.signature,
        crate::sign::AnySignature::Ed25519ph(_)
    ));
    client.finish(&with_proof(body, &proof)).await.unwrap();
}

#[tokio::test]
async fn initiation_is_fresh_for_each_connection() {
    let first = client(&signer(5)).await;
    let second = client(&signer(5)).await;
    assert_ne!(first.message(), second.message());
    let (_, a, _) = decode(first.message(), &["server.test".into()])
        .await
        .unwrap();
    let (_, b, _) = decode(second.message(), &["server.test".into()])
        .await
        .unwrap();
    assert_ne!(a.commitment.nonce, b.commitment.nonce);
}

#[tokio::test]
async fn full_width_nonce_reaches_the_existing_replay_policy_without_precision_loss() {
    let key = signer(6);
    let mut client = client(&key).await;
    let (body, mut proof) = split_proof(client.message());
    proof.commitment.nonce = u64::MAX;
    proof.signature = key
        .sign_commitment(&proof.commitment, "server.test")
        .await
        .unwrap();
    client.message = with_proof(body, &proof);
    let (_, _, response, _) = accept(
        &signer(7),
        &["server.test".into()],
        client.message(),
        &Target::Rpc,
        None,
        |proof| async move {
            assert_eq!(proof.nonce, u64::MAX);
            Ok(())
        },
    )
    .await
    .unwrap();
    client.finish(&response).await.unwrap();
}

#[tokio::test]
async fn server_response_cannot_be_replayed_into_another_connection() {
    let first = client(&signer(8)).await;
    let second = client(&signer(8)).await;
    let (_, response, _) = accept_rpc(&signer(9), first.message()).await;
    assert!(second.finish(&response).await.is_err());
    first.finish(&response).await.unwrap();
}

#[tokio::test]
async fn context_role_and_protocol_are_authenticated() {
    let key = signer(10);
    let client = client(&key).await;
    reject_before_nonce(client.message(), "other.test", None).await;
    for changed in [
        resign(client.message(), &key, |message| message.version += 1).await,
        resign(client.message(), &key, |message| {
            message.protocol = "other protocol".into()
        })
        .await,
        resign(client.message(), &key, |message| {
            message.hello = Hello::Server {
                ephemeral: ByteString([1; 32]),
                client_hello: ByteString([0; 32]),
            }
        })
        .await,
    ] {
        reject_before_nonce(&changed, "server.test", None).await;
    }
}

#[tokio::test]
async fn expected_peer_and_contributory_dh_are_checked_before_nonce_policy() {
    let key = signer(12);
    let client = client(&key).await;
    reject_before_nonce(
        client.message(),
        "server.test",
        Some(&signer(14).verifying_key().await.unwrap()),
    )
    .await;
    let low_order = resign(client.message(), &key, |message| {
        message.hello = Hello::Client {
            ephemeral: ByteString([0; 32]),
            target: Target::Rpc,
        };
    })
    .await;
    reject_before_nonce(&low_order, "server.test", None).await;
}

#[tokio::test]
async fn signed_intent_cannot_be_redirected_between_rpc_and_continuations_or_between_guids() {
    let key = signer(22);
    let peer = key.verifying_key().await.unwrap();
    let a = Target::Continuation(Guid::new());
    let b = Target::Continuation(Guid::new());
    for (intent, actual) in [
        (Target::Rpc, a.clone()),
        (a.clone(), Target::Rpc),
        (a.clone(), b),
    ] {
        let client = PreparedClient::new(&key, "server.test", intent)
            .await
            .unwrap();
        assert!(
            accept(
                &signer(23),
                &["server.test".into()],
                client.message(),
                &actual,
                Some(&peer),
                |_| async { panic!("wrong target reached nonce policy") }
            )
            .await
            .is_err()
        );
    }
    let client = PreparedClient::new(&key, "server.test", a.clone())
        .await
        .unwrap();
    let (_, _, response, mut server) = accept(
        &signer(23),
        &["server.test".into()],
        client.message(),
        &a,
        Some(&peer),
        |_| async { Ok(()) },
    )
    .await
    .unwrap();
    let (_, mut client) = client.finish(&response).await.unwrap();
    let mut plain = [0; 64];
    let n = client
        .read_message(
            &encrypted(&mut server, b"server-first continuation"),
            &mut plain,
        )
        .unwrap();
    assert_eq!(&plain[..n], b"server-first continuation");
}

#[tokio::test]
async fn replay_policy_rejection_cannot_produce_a_session() {
    let client = client(&signer(15)).await;
    assert!(
        accept(
            &signer(16),
            &["server.test".into()],
            client.message(),
            &Target::Rpc,
            None,
            |_| async { Err(invalid()) }
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn body_and_identity_substitution_fail_proof_verification() {
    let client = client(&signer(17)).await;
    let (body, mut proof) = split_proof(client.message());
    let mut reordered: serde_cbor::Value = serde_cbor::de::from_reader(body).unwrap();
    let serde_cbor::Value::Map(fields) = &mut reordered else {
        panic!("expected body map")
    };
    fields.reverse();
    let mut changed = Vec::new();
    serde_cbor::ser::into_writer(&reordered, &mut changed).unwrap();
    assert_ne!(changed, body);
    reject_before_nonce(&with_proof(&changed, &proof), "server.test", None).await;
    proof.signer = signer(19).verifying_key().await.unwrap();
    reject_before_nonce(&with_proof(body, &proof), "server.test", None).await;
}

#[tokio::test]
async fn record_authentication_rejects_tampering_replay_reflection_and_reordering() {
    let mut plain = [0; 64];
    let (mut client, mut server) = pair().await;
    let _first = encrypted(&mut client, b"first");
    assert!(
        server
            .read_message(&encrypted(&mut client, b"second"), &mut plain)
            .is_err()
    );
    let (mut client, _) = pair().await;
    let reflected = encrypted(&mut client, b"first");
    assert!(client.read_message(&reflected, &mut plain).is_err());
    let (mut client, mut server) = pair().await;
    let mut tampered = encrypted(&mut client, b"first");
    tampered[0] ^= 1;
    assert!(server.read_message(&tampered, &mut plain).is_err());
    let (mut client, mut server) = pair().await;
    let first = encrypted(&mut client, b"first");
    assert_eq!(server.read_message(&first, &mut plain).unwrap(), 5);
    assert!(server.read_message(&first, &mut plain).is_err());
}

#[tokio::test]
async fn counter_exhaustion_fails_before_nonce_reuse_and_short_buffers_are_rejected() {
    let (mut client, mut server) = pair().await;
    assert!(client.write_message(&[], &mut []).is_err());
    client.send.counter = MAX_RECORDS_PER_KEY - 1;
    server.receive.counter = MAX_RECORDS_PER_KEY - 1;
    let record = encrypted(&mut client, b"last");
    assert_eq!(server.read_message(&record, &mut [0; 32]).unwrap(), 4);
    assert!(client.write_message(b"more", &mut [0; 32]).is_err());
    assert!(server.read_message(&record, &mut [0; 32]).is_err());
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

fn fixture_client(fixture: &WebCryptoFixture) -> PreparedClient {
    PreparedClient {
        secret: StaticSecret::from(
            <[u8; 32]>::try_from(hex::decode(&fixture.client_private).unwrap()).unwrap(),
        ),
        context: fixture.context.as_str().into(),
        message: fixture.client.0.clone(),
    }
}

fn fixture() -> WebCryptoFixture {
    serde_json::from_str(include_str!(
        "../../../testdata/signed-session/webcrypto.json"
    ))
    .unwrap()
}

#[tokio::test]
async fn independent_webcrypto_proofs_dh_hkdf_and_aes_gcm_match_in_both_directions() {
    let fixture = fixture();
    let (peer, mut cipher) = fixture_client(&fixture)
        .finish(&fixture.server)
        .await
        .unwrap();
    assert_eq!(peer, fixture.server_identity);
    assert_eq!(
        encrypted(&mut cipher, fixture.client_record.plaintext.as_bytes()),
        fixture.client_record.ciphertext.0
    );
    let mut plain = [0; 128];
    let n = cipher
        .read_message(&fixture.server_record.ciphertext, &mut plain)
        .unwrap();
    assert_eq!(&plain[..n], fixture.server_record.plaintext.as_bytes());
}

#[tokio::test]
async fn native_server_accepts_webcrypto_identity_and_request_commitment() {
    let fixture = fixture();
    let (peer, _, response, mut server) = accept(
        &signer(21),
        &[fixture.context.as_str().into()],
        &fixture.client,
        &Target::Rpc,
        None,
        |proof| async move {
            assert_eq!(proof.nonce, u64::MAX);
            Ok(())
        },
    )
    .await
    .unwrap();
    assert_eq!(peer, fixture.client_identity);
    let (_, mut client) = fixture_client(&fixture).finish(&response).await.unwrap();
    let ciphertext = encrypted(&mut client, b"WebCrypto initiation with native server");
    let mut plain = [0; 128];
    let n = server.read_message(&ciphertext, &mut plain).unwrap();
    assert_eq!(&plain[..n], b"WebCrypto initiation with native server");
}

#[tokio::test]
async fn handshake_is_two_cbor_maps_with_binary_key_material_and_no_outer_envelope() {
    fn field<'a>(value: &'a serde_cbor::Value, name: &str) -> &'a serde_cbor::Value {
        &value
            .as_map()
            .unwrap()
            .iter()
            .find(|(key, _)| key.as_text() == Some(name))
            .unwrap()
            .1
    }
    let client = client(&signer(27)).await;
    let mut reader = std::io::Cursor::new(client.message());
    let body: serde_cbor::Value = serde_cbor::de::from_reader(&mut reader).unwrap();
    let proof: serde_cbor::Value = serde_cbor::de::from_reader(&mut reader).unwrap();
    assert_eq!(reader.position() as usize, client.message().len());
    assert_eq!(body.as_map().unwrap().len(), 3);
    assert_eq!(proof.as_map().unwrap().len(), 3);
    assert_eq!(
        field(field(&body, "hello"), "ephemeral")
            .as_bytes()
            .unwrap()
            .len(),
        32
    );
    assert_eq!(
        field(field(&proof, "commitment"), "blake3")
            .as_bytes()
            .unwrap()
            .len(),
        32
    );
    assert_eq!(
        field(&proof, "signer").as_bytes().unwrap(),
        &signer(27)
            .verifying_key()
            .await
            .unwrap()
            .to_der::<serde_json::Error>()
            .unwrap()
    );
    assert!(field(&proof, "signature").as_bytes().is_some());
}

#[tokio::test]
async fn extra_objects_unknown_fields_and_duplicate_fields_fail_before_nonce_policy() {
    let key = signer(28);
    let client = client(&key).await;
    let mut trailing = client.message().to_vec();
    trailing.push(0xf6);
    reject_before_nonce(&trailing, "server.test", None).await;
    let (body, _) = split_proof(client.message());
    for duplicate in [false, true] {
        let mut value: serde_cbor::Value = serde_cbor::de::from_reader(body).unwrap();
        let serde_cbor::Value::Map(fields) = &mut value else {
            panic!("expected body map")
        };
        fields.push((
            serde_cbor::Value::Text(if duplicate { "protocol" } else { "unexpected" }.into()),
            serde_cbor::Value::Text("Start-RPC-Session".into()),
        ));
        let mut body = Vec::new();
        serde_cbor::ser::into_writer(&value, &mut body).unwrap();
        let proof = RequestProof::sign_commitment(&key, &body, "server.test")
            .await
            .unwrap();
        reject_before_nonce(&with_proof(&body, &proof), "server.test", None).await;
    }
}

#[tokio::test]
async fn malformed_and_oversized_envelopes_do_not_reach_replay_policy() {
    for wire in [
        vec![0; MAX_HANDSHAKE_BYTES + 1],
        b"{\"payload\":\"bad\",\"proof\":\"\"}".to_vec(),
        b"{}".to_vec(),
    ] {
        reject_before_nonce(&wire, "server.test", None).await;
    }
    assert_eq!(wire_profile()["version"], VERSION);
}
