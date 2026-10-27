use ed25519_dalek::VerifyingKey;
use pgp::composed::{Deserializable, SignedPublicKey};
use pgp::types::{EddsaLegacyPublicParams, KeyDetails, PublicParams};

use crate::prelude::*;
use crate::sign::AnyVerifyingKey;

fn invalid_key() -> Error {
    Error::new(
        eyre!("{}", t!("sign.gpg.invalid-public-key")),
        ErrorKind::InvalidRequest,
    )
}

pub(crate) fn parse_public_keys(values: &[String]) -> Result<Vec<AnyVerifyingKey>, Error> {
    let mut keys = Vec::new();
    for value in values {
        if value
            .trim_start()
            .starts_with("-----BEGIN PGP PUBLIC KEY BLOCK-----")
        {
            keys.extend(parse_gpg_keys(value)?);
        } else {
            keys.push(value.parse()?);
        }
    }
    Ok(keys)
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

fn parse_gpg_keys(value: &str) -> Result<Vec<AnyVerifyingKey>, Error> {
    let (certificates, _) =
        SignedPublicKey::from_armor_many(value.as_bytes()).with_kind(ErrorKind::Deserialization)?;
    let mut keys = Vec::new();
    for certificate in certificates {
        let certificate = certificate.with_kind(ErrorKind::Deserialization)?;
        if let Some(key) = ed25519_public_key(certificate.primary_key.public_params())? {
            keys.push(key);
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
                keys.push(key);
            }
        }
    }
    if keys.is_empty() {
        return Err(invalid_key());
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update::signers::initial_signers;

    #[test]
    fn gpg_public_export_matches_existing_ed25519_identity() {
        let keys = parse_public_keys(&[include_str!("fixtures/start9-public.asc").into()]).unwrap();
        let expected = initial_signers()
            .into_iter()
            .find(|(_, info)| info.label == "Start9")
            .unwrap()
            .0
            .parse::<AnyVerifyingKey>()
            .unwrap();
        assert_eq!(keys, [expected]);
    }

    #[test]
    fn imports_ed25519_signing_keys_without_encryption_subkeys() {
        let keys =
            parse_public_keys(&[include_str!("fixtures/ed25519-signers.asc").into()]).unwrap();
        assert_eq!(keys.len(), 2);
        assert_ne!(keys[0], keys[1]);
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
        let pem = initial_signers().into_keys().next().unwrap();
        let keys = parse_public_keys(&[
            pem.to_string(),
            include_str!("fixtures/start9-public.asc").into(),
        ])
        .unwrap();
        assert_eq!(keys[0], pem.parse::<AnyVerifyingKey>().unwrap());
        assert_eq!(keys.len(), 2);
        assert!(parse_public_keys(&["not a public key".into()]).is_err());
    }
}
