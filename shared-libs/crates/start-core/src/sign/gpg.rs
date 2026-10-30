use std::collections::BTreeSet;
use std::io::Read;

use ed25519_dalek::VerifyingKey;
use pgp::armor::{BlockType, Dearmor};
use pgp::composed::{Deserializable, PacketBodyReader, SignedPublicKey};
use pgp::packet::{PacketHeader, SignatureType};
use pgp::types::{EddsaLegacyPublicParams, KeyDetails, PublicParams, Tag};
use serde::{Deserialize, Serialize};

use crate::prelude::*;
use crate::sign::AnyVerifyingKey;

/// An original public-key export and its self-certified identity claims.
#[derive(
    Clone, Debug, Deserialize, Serialize, HasModel, VisitFields, PartialEq, Eq, PartialOrd, Ord,
)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct GpgPublicKeyExport {
    /// Original armor, preserving all certificate packets and headers.
    pub armored: String,
    /// Primary-key fingerprints of the certificates in this export.
    pub fingerprints: BTreeSet<String>,
    /// Self-certified user IDs containing names, comments, and email addresses.
    pub user_ids: BTreeSet<String>,
}

rpc_toolkit::reflect_ts!(GpgPublicKeyExport);
rpc_toolkit::ts_export!(GpgPublicKeyExport, namespaces = [""]);

#[derive(Debug, Default)]
pub struct ImportedPublicKeys {
    pub keys: BTreeSet<AnyVerifyingKey>,
    pub gpg_exports: BTreeSet<GpgPublicKeyExport>,
}

impl From<Vec<AnyVerifyingKey>> for ImportedPublicKeys {
    fn from(keys: Vec<AnyVerifyingKey>) -> Self {
        Self {
            keys: keys.into_iter().collect(),
            gpg_exports: BTreeSet::new(),
        }
    }
}

fn invalid_key() -> Error {
    Error::new(
        eyre!("{}", t!("sign.gpg.invalid-public-key")),
        ErrorKind::InvalidRequest,
    )
}

pub(crate) fn parse_public_keys(values: &[String]) -> Result<ImportedPublicKeys, Error> {
    let mut imported = ImportedPublicKeys::default();
    for value in values {
        if value
            .trim_start()
            .starts_with("-----BEGIN PGP PUBLIC KEY BLOCK-----")
        {
            let gpg = parse_gpg_keys(value)?;
            imported.keys.extend(gpg.keys);
            imported.gpg_exports.extend(gpg.gpg_exports);
        } else {
            imported.keys.insert(value.parse()?);
        }
    }
    Ok(imported)
}

fn ed25519_public_key(params: &PublicParams) -> Result<Option<AnyVerifyingKey>, Error> {
    let bytes = match params {
        PublicParams::EdDSALegacy(EddsaLegacyPublicParams::Ed25519 { key }) => key.to_bytes(),
        PublicParams::Ed25519(params) => params.key.to_bytes(),
        _ => return Ok(None),
    };
    Ok(Some(AnyVerifyingKey::Ed25519(VerifyingKey::from_bytes(
        &bytes,
    )?)))
}

fn parse_gpg_keys(value: &str) -> Result<ImportedPublicKeys, Error> {
    let mut imported = ImportedPublicKeys::default();
    let mut export = GpgPublicKeyExport {
        armored: value.to_owned(),
        fingerprints: BTreeSet::new(),
        user_ids: BTreeSet::new(),
    };
    for certificate in public_certificates(value)? {
        let revoked = primary_revoked(&certificate)?;
        export
            .fingerprints
            .insert(certificate.fingerprint().to_string());
        export
            .user_ids
            .extend(self_certified_user_ids(&certificate));
        if revoked {
            continue;
        }
        if let Some(key) = ed25519_public_key(certificate.primary_key.public_params())? {
            imported.keys.insert(key);
        }
        for subkey in &certificate.public_subkeys {
            if !subkey
                .signatures
                .iter()
                .any(|signature| signature.key_flags().sign())
            {
                continue;
            }
            if let Some(key) = ed25519_public_key(subkey.public_params())? {
                subkey
                    .verify_bindings(&certificate.primary_key)
                    .with_kind(ErrorKind::InvalidSignature)?;
                if !subkey
                    .signatures
                    .iter()
                    .any(|signature| signature.typ() == Some(SignatureType::SubkeyRevocation))
                {
                    imported.keys.insert(key);
                }
            }
        }
    }
    if imported.keys.is_empty() {
        return Err(invalid_key());
    }
    imported.gpg_exports.insert(export);
    Ok(imported)
}

fn public_certificates(value: &str) -> Result<Vec<SignedPublicKey>, Error> {
    let mut armor = Dearmor::new(value.as_bytes());
    armor.read_header().with_kind(ErrorKind::Deserialization)?;
    if armor.typ != Some(BlockType::PublicKey) {
        return Err(invalid_key());
    }
    let mut body = Vec::new();
    armor
        .read_to_end(&mut body)
        .with_kind(ErrorKind::Deserialization)?;
    let footer = format!("\n-----END {}-----", BlockType::PublicKey);
    let (_, trailing) = value.split_once(&footer).ok_or_else(invalid_key)?;
    if !trailing.trim().is_empty() {
        return Err(invalid_key());
    }
    let mut remaining = body.as_slice();
    while !remaining.is_empty() {
        let header =
            PacketHeader::try_from_reader(&mut remaining).with_kind(ErrorKind::Deserialization)?;
        if matches!(header.tag(), Tag::SecretKey | Tag::SecretSubkey) {
            return Err(invalid_key());
        }
        let mut packet =
            PacketBodyReader::new(header, &mut remaining).with_kind(ErrorKind::Deserialization)?;
        std::io::copy(&mut packet, &mut std::io::sink()).with_kind(ErrorKind::Deserialization)?;
    }
    SignedPublicKey::from_bytes_many(body.as_slice())
        .with_kind(ErrorKind::Deserialization)?
        .collect::<pgp::errors::Result<Vec<_>>>()
        .with_kind(ErrorKind::Deserialization)
}

fn self_certified_user_ids(certificate: &SignedPublicKey) -> BTreeSet<String> {
    certificate
        .details
        .users
        .iter()
        .filter(|user| {
            user.signatures.iter().any(|signature| {
                signature
                    .verify_certification(&certificate.primary_key, Tag::UserId, &user.id)
                    .is_ok()
            })
        })
        .filter_map(|user| user.id.as_str().map(str::to_owned))
        .collect()
}

/// Errors on a revocation signature the primary key did not make.
fn primary_revoked(certificate: &SignedPublicKey) -> Result<bool, Error> {
    for signature in &certificate.details.revocation_signatures {
        signature
            .verify_key(&certificate.primary_key)
            .with_kind(ErrorKind::InvalidSignature)?;
    }
    Ok(!certificate.details.revocation_signatures.is_empty())
}

#[cfg(test)]
mod tests {
    use pgp::packet::{Packet, PacketParser};
    use pgp::ser::Serialize as _;

    use super::*;
    use crate::update::signers::initial_signers;

    fn armored(source: &impl pgp::ser::Serialize) -> String {
        let mut result = Vec::new();
        pgp::armor::write(source, BlockType::PublicKey, &mut result, None, false).unwrap();
        String::from_utf8(result).unwrap()
    }

    #[test]
    fn retains_original_exports_including_headers_expiry_subkeys_and_revocations() {
        for source in [
            include_str!("../trusted_keys/initial/matthill.asc"),
            include_str!("fixtures/revoked-subkey.asc"),
        ] {
            let source = source.replacen(
                "-----BEGIN PGP PUBLIC KEY BLOCK-----",
                "-----BEGIN PGP PUBLIC KEY BLOCK-----\nComment: Retained export",
                1,
            );
            let imported = parse_public_keys(&[source.clone(), source.clone()]).unwrap();
            assert_eq!(imported.gpg_exports.len(), 1);
            let saved = serde_json::to_string(&imported.gpg_exports).unwrap();
            let restored: BTreeSet<GpgPublicKeyExport> = serde_json::from_str(&saved).unwrap();
            assert_eq!(restored, imported.gpg_exports);
            let export = restored.first().unwrap();
            assert_eq!(export.armored, source);
            assert_eq!(export.fingerprints.len(), 1);
            assert!(!export.user_ids.is_empty());
        }
        let imported =
            parse_public_keys(&[include_str!("../trusted_keys/initial/matthill.asc").into()])
                .unwrap();
        let export = imported.gpg_exports.first().unwrap();
        assert_eq!(
            export.user_ids,
            BTreeSet::from(["Matt Hill <matthill@start9.com>".into()])
        );
        let (certificate, _) =
            SignedPublicKey::from_armor_single(export.armored.as_bytes()).unwrap();
        assert_eq!(certificate.public_subkeys.len(), 1);
        assert!(
            certificate.details.users[0]
                .signatures
                .iter()
                .any(|signature| signature.key_expiration_time().is_some())
        );
        assert_eq!(imported.keys.len(), 1);
    }

    #[test]
    fn retains_all_certificates_in_a_single_export_without_trusting_revoked_primaries() {
        let certificates = [
            SignedPublicKey::from_armor_single(
                include_str!("fixtures/start9-public.asc").as_bytes(),
            )
            .unwrap()
            .0,
            SignedPublicKey::from_armor_single(
                include_str!("fixtures/revoked-primary.asc").as_bytes(),
            )
            .unwrap()
            .0,
        ];
        let source = armored(&certificates.as_slice());
        let imported = parse_public_keys(&[source.clone()]).unwrap();
        let export = imported.gpg_exports.first().unwrap();
        assert_eq!(export.armored, source);
        assert_eq!(export.fingerprints.len(), 2);
        assert_eq!(imported.keys.len(), 1);
    }

    #[test]
    fn retains_unsupported_public_metadata_without_changing_signing_trust() {
        use base64::Engine as _;
        use pgp::packet::PacketHeader;
        use pgp::types::{ExperimentalTag, PacketHeaderVersion, PacketLength};

        let (certificate, _) = SignedPublicKey::from_armor_single(
            include_str!("fixtures/start9-public.asc").as_bytes(),
        )
        .unwrap();
        let mut bytes = certificate.to_bytes().unwrap();
        PacketHeader::from_parts(
            PacketHeaderVersion::New,
            Tag::Experimental(ExperimentalTag::new(60).unwrap()),
            PacketLength::Fixed(3),
        )
        .unwrap()
        .to_writer(&mut bytes)
        .unwrap();
        bytes.extend_from_slice(b"abc");
        let source = format!(
            "-----BEGIN PGP PUBLIC KEY BLOCK-----\n\n{}\n-----END PGP PUBLIC KEY BLOCK-----\n",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        );
        let (certificates, _) = SignedPublicKey::from_armor_many(source.as_bytes()).unwrap();
        assert_eq!(
            certificates
                .collect::<pgp::errors::Result<Vec<_>>>()
                .unwrap()
                .len(),
            1
        );
        let imported = parse_public_keys(&[source.clone()]).unwrap();
        assert_eq!(
            imported.keys,
            BTreeSet::from([
                "MCowBQYDK2VwAyEAJdDg62o3oHIh4lXECn9gqORe+cCBbH/67MWjMxIMzDg="
                    .parse()
                    .unwrap()
            ])
        );
        assert_eq!(imported.gpg_exports.first().unwrap().armored, source);
    }

    #[test]
    fn identity_claims_require_a_matching_self_certification() {
        let (mut certificate, _) = SignedPublicKey::from_armor_single(
            include_str!("fixtures/start9-public.asc").as_bytes(),
        )
        .unwrap();
        certificate.details.users[0].id = pgp::packet::UserId::from_str(
            pgp::types::PacketHeaderVersion::New,
            "Impostor <impostor@example.test>",
        )
        .unwrap();
        let source = certificate.to_armored_string(Default::default()).unwrap();
        let imported = parse_public_keys(&[source.clone()]).unwrap();
        let export = imported.gpg_exports.first().unwrap();
        assert_eq!(export.armored, source);
        assert!(!export.user_ids.contains("Impostor <impostor@example.test>"));
        assert_eq!(imported.keys.len(), 1);
    }

    #[test]
    fn original_export_retention_rejects_private_packets_and_unparsed_suffixes() {
        let source = include_str!("fixtures/start9-public.asc");
        for suffix in [
            "unparsed data",
            "-----BEGIN PGP PRIVATE KEY BLOCK-----\nprivate\n-----END PGP PRIVATE KEY BLOCK-----",
            source,
        ] {
            assert!(
                parse_public_keys(&[format!("{source}{suffix}")]).is_err(),
                "accepted suffix: {suffix}"
            );
        }
        assert!(
            parse_public_keys(&[source.replace("-----END PGP PUBLIC KEY BLOCK-----", "")]).is_err()
        );
        let (certificate, _) = SignedPublicKey::from_armor_single(source.as_bytes()).unwrap();
        let bytes = certificate.to_bytes().unwrap();
        let mut packets = PacketParser::new(bytes.as_slice())
            .collect::<pgp::errors::Result<Vec<_>>>()
            .unwrap();
        let private = pgp::crypto::ed25519::SecretKey::try_from_bytes(
            [42; 32],
            pgp::crypto::ed25519::Mode::EdDSALegacy,
        )
        .unwrap();
        let secret = pgp::packet::SecretKey::new(
            certificate.primary_key,
            pgp::types::SecretParams::Plain(pgp::types::PlainSecretParams::EdDSALegacy(
                pgp::crypto::eddsa_legacy::SecretKey::Ed25519(private),
            )),
        )
        .unwrap();
        packets.push(Packet::SecretKey(secret));
        assert_eq!(
            parse_public_keys(&[armored(&packets)]).unwrap_err().kind,
            ErrorKind::InvalidRequest
        );
    }

    #[test]
    fn gpg_public_export_matches_existing_ed25519_identity() {
        let keys = parse_public_keys(&[include_str!("fixtures/start9-public.asc").into()])
            .unwrap()
            .keys;
        let expected = initial_signers()
            .into_iter()
            .find(|(_, info)| info.label == "Start9")
            .unwrap()
            .0
            .parse::<AnyVerifyingKey>()
            .unwrap();
        assert_eq!(keys, BTreeSet::from([expected]));
    }

    #[test]
    fn imports_ed25519_signing_keys_without_encryption_subkeys() {
        let keys = parse_public_keys(&[include_str!("fixtures/ed25519-signers.asc").into()])
            .unwrap()
            .keys;
        assert_eq!(keys.len(), 2);
        for key in keys {
            assert_eq!(key.to_string().parse::<AnyVerifyingKey>().unwrap(), key);
        }
    }

    #[test]
    fn legacy_and_modern_ed25519_encodings_resolve_to_the_same_identity() {
        let (certificate, _) = SignedPublicKey::from_armor_single(
            include_str!("fixtures/start9-public.asc").as_bytes(),
        )
        .unwrap();
        let legacy = certificate.primary_key.public_params();
        let PublicParams::EdDSALegacy(EddsaLegacyPublicParams::Ed25519 { key }) = legacy else {
            panic!("fixture must use legacy Ed25519")
        };
        let modern = PublicParams::Ed25519(pgp::types::Ed25519PublicParams { key: *key });
        assert_eq!(
            ed25519_public_key(legacy).unwrap(),
            ed25519_public_key(&modern).unwrap()
        );
    }

    #[test]
    fn rejects_a_signing_subkey_with_a_mismatched_binding() {
        let (mut certificate, _) = SignedPublicKey::from_armor_single(
            include_str!("fixtures/ed25519-signers.asc").as_bytes(),
        )
        .unwrap();
        let replacement = certificate.primary_key.public_params().clone();
        let subkey = &mut certificate.public_subkeys[0];
        let inner = pgp::packet::PubKeyInner::new(
            subkey.key.version(),
            subkey.key.algorithm(),
            subkey.key.created_at(),
            None,
            replacement,
        )
        .unwrap();
        subkey.key = pgp::packet::PublicSubkey::from_inner(inner).unwrap();
        let armored = certificate.to_armored_string(Default::default()).unwrap();
        assert!(parse_public_keys(&[armored]).is_err());
    }

    #[test]
    fn skips_revoked_signing_keys() {
        let keys = parse_public_keys(&[include_str!("fixtures/revoked-subkey.asc").into()])
            .unwrap()
            .keys;
        assert_eq!(keys.len(), 2);
        let (certificate, _) = SignedPublicKey::from_armor_single(
            include_str!("fixtures/revoked-subkey.asc").as_bytes(),
        )
        .unwrap();
        let revoked = &certificate.public_subkeys[0];
        assert!(
            revoked
                .signatures
                .iter()
                .any(|signature| signature.typ() == Some(SignatureType::SubkeyRevocation))
        );
        let revoked = ed25519_public_key(revoked.public_params())
            .unwrap()
            .unwrap();
        assert!(!keys.contains(&revoked));
        assert!(parse_public_keys(&[include_str!("fixtures/revoked-primary.asc").into()]).is_err());
    }

    #[test]
    fn rejects_forged_revocations_on_primary_keys() {
        let (mut certificate, _) = SignedPublicKey::from_armor_single(
            include_str!("fixtures/ed25519-signers.asc").as_bytes(),
        )
        .unwrap();
        let (revoked, _) = SignedPublicKey::from_armor_single(
            include_str!("fixtures/revoked-primary.asc").as_bytes(),
        )
        .unwrap();
        certificate.details.revocation_signatures = revoked.details.revocation_signatures;
        let armored = certificate.to_armored_string(Default::default()).unwrap();
        assert_eq!(
            parse_public_keys(&[armored]).unwrap_err().kind,
            ErrorKind::InvalidSignature
        );
    }

    #[test]
    fn rejects_forged_revocations_on_subkeys() {
        let (mut certificate, _) = SignedPublicKey::from_armor_single(
            include_str!("fixtures/revoked-subkey.asc").as_bytes(),
        )
        .unwrap();
        let revocation = certificate.public_subkeys[0]
            .signatures
            .iter()
            .find(|signature| signature.typ() == Some(SignatureType::SubkeyRevocation))
            .unwrap()
            .clone();
        certificate.public_subkeys[1].signatures.push(revocation);
        let armored = certificate.to_armored_string(Default::default()).unwrap();
        assert_eq!(
            parse_public_keys(&[armored]).unwrap_err().kind,
            ErrorKind::InvalidSignature
        );
    }

    #[test]
    fn rejects_unsupported_malformed_and_private_exports() {
        for input in [
            include_str!("fixtures/rsa-public.asc"),
            "-----BEGIN PGP PUBLIC KEY BLOCK-----\ninvalid\n-----END PGP PUBLIC KEY BLOCK-----",
            "-----BEGIN PGP PRIVATE KEY BLOCK-----\ninvalid\n-----END PGP PRIVATE KEY BLOCK-----",
        ] {
            assert!(parse_public_keys(&[input.into()]).is_err());
        }
    }

    #[test]
    fn preserves_pem_inputs_and_accepts_mixed_formats() {
        let pem = initial_signers()
            .into_iter()
            .find(|(_, signer)| signer.label == "matthill")
            .unwrap()
            .0;
        let keys = parse_public_keys(&[
            pem.to_string(),
            include_str!("fixtures/start9-public.asc").into(),
        ])
        .unwrap();
        assert!(keys.keys.contains(&pem.parse::<AnyVerifyingKey>().unwrap()));
        assert_eq!(keys.keys.len(), 2);
        assert_eq!(keys.gpg_exports.len(), 1);
        assert!(parse_public_keys(&["not a public key".into()]).is_err());
    }
}
