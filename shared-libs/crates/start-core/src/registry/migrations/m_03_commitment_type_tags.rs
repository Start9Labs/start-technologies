use std::path::Path;

use super::RegistryMigration;
use crate::prelude::*;
use crate::sign::commitment::SignedCommitment;
use crate::sign::commitment::blake3::Blake3Commitment;
use crate::sign::commitment::merkle_archive::MerkleArchiveCommitment;

pub struct CommitmentTypeTags;
impl RegistryMigration for CommitmentTypeTags {
    fn action(&self, db: &mut Value, _: &Path) -> Result<(), Error> {
        tag_commitments(&mut db["index"]);
        Ok(())
    }
}

/// Adds the `type` tag to every stored asset commitment that lacks one.
fn tag_commitments(value: &mut Value) {
    match value {
        Value::Object(obj) => {
            for (k, v) in obj.iter_mut() {
                match (&**k, v) {
                    ("commitment", Value::Object(c)) if !c.contains_key("type") => {
                        let name = if c.contains_key("rootSighash") {
                            MerkleArchiveCommitment::TYPE_NAME
                        } else {
                            Blake3Commitment::TYPE_NAME
                        };
                        c.insert("type".into(), Value::String(name.to_owned().into()));
                    }
                    (_, v) => tag_commitments(v),
                }
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                tag_commitments(v);
            }
        }
        _ => (),
    }
}

#[cfg(test)]
mod tests {
    use imbl_value::json;

    use super::*;

    #[test]
    fn tags_os_and_package_commitments() {
        let mut db = json!({ "index": {
            "os": { "versions": { "0.4.0.2": { "squashfs": { "x86_64": {
                "commitment": { "hash": "h", "size": 1 },
            } } } } },
            "package": { "packages": { "foo": { "versions": { "1.0.0": { "s9pks": [[{}, {
                "commitment": { "rootSighash": "r", "rootMaxsize": 1 },
            }]] } } } } },
        } });
        CommitmentTypeTags.action(&mut db, Path::new("")).unwrap();
        assert_eq!(
            db["index"]["os"]["versions"]["0.4.0.2"]["squashfs"]["x86_64"]["commitment"]["type"],
            json!("blake3")
        );
        assert_eq!(
            db["index"]["package"]["packages"]["foo"]["versions"]["1.0.0"]["s9pks"][0][1]["commitment"]
                ["type"],
            json!("merkle-archive")
        );
    }
}
