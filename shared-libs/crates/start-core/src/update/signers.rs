use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::prelude::*;
pub use crate::trusted_keys::initial_signers;

pub type TrustedOsUpdateSigners = BTreeMap<InternedString, TrustedOsUpdateSigner>;

#[derive(Clone, Debug, Deserialize, Serialize, HasModel, VisitFields, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct TrustedOsUpdateSigner {
    pub label: String,
}

rpc_toolkit::reflect_ts!(TrustedOsUpdateSigner);
rpc_toolkit::ts_export!(TrustedOsUpdateSigner, namespaces = [""]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sign::AnyVerifyingKey;

    fn get_signer(
        signers: &TrustedOsUpdateSigners,
        key: &AnyVerifyingKey,
    ) -> Option<TrustedOsUpdateSigner> {
        signers.get(&key.interned_der64()).cloned()
    }

    #[test]
    fn initial_trust_contains_only_the_three_approved_keys() {
        let signers = initial_signers();
        assert_eq!(signers.len(), 3);
        for (label, body) in [
            (
                "drbonez",
                "MCowBQYDK2VwAyEAgpcI3hOExv1njCt+zJoYBartdyk4SKiqSIy3+9tcpOI=",
            ),
            (
                "matthill",
                "MCowBQYDK2VwAyEAfeubYt/ozIfZspUZdmu2Fa6sK2psMnHVGxuVolo8u40=",
            ),
            (
                "Start9",
                "MCowBQYDK2VwAyEAJdDg62o3oHIh4lXECn9gqORe+cCBbH/67MWjMxIMzDg=",
            ),
        ] {
            let key: AnyVerifyingKey =
                format!("-----BEGIN PUBLIC KEY-----\n{body}\n-----END PUBLIC KEY-----\n")
                    .parse()
                    .unwrap();
            assert_eq!(get_signer(&signers, &key).unwrap().label, label);
        }
        for body in [
            "MCowBQYDK2VwAyEAqPjOeMD2CB3UEASoICgfJlfrO5ygjOSBCCNRyDBs75A=",
            "MCowBQYDK2VwAyEASv7fB+rs6FRL+pk7KXaoXONNn26dRkwMB9LXUqmXXLM=",
        ] {
            let key: AnyVerifyingKey =
                format!("-----BEGIN PUBLIC KEY-----\n{body}\n-----END PUBLIC KEY-----\n")
                    .parse()
                    .unwrap();
            assert!(get_signer(&signers, &key).is_none());
        }
    }

    #[test]
    fn initial_start9_key_matches_repository_openpgp_primary_key() {
        let home = tempfile::tempdir().unwrap();
        let output = std::process::Command::new("gpg")
            .arg("--homedir")
            .arg(home.path())
            .args([
                "--batch",
                "--no-options",
                "--with-colons",
                "--with-key-data",
                "--show-keys",
                concat!(env!("CARGO_MANIFEST_DIR"), "/../../../apt/start9.gpg"),
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let records = String::from_utf8(output.stdout).unwrap();
        let primary = records
            .lines()
            .find(|line| line.starts_with("pub:"))
            .unwrap();
        assert_eq!(primary.split(':').nth(3), Some("22"));
        let point = records
            .lines()
            .find(|line| line.starts_with("pkd:1:"))
            .unwrap()
            .split(':')
            .nth(3)
            .unwrap();
        let bytes = hex::decode(point.strip_prefix("40").unwrap()).unwrap();
        let key = AnyVerifyingKey::Ed25519(
            ed25519_dalek::VerifyingKey::from_bytes(&bytes.try_into().unwrap()).unwrap(),
        );
        assert_eq!(
            get_signer(&initial_signers(), &key).unwrap().label,
            "Start9"
        );
    }
}
