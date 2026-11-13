use std::collections::BTreeMap;
use std::path::Path;

use super::RegistryMigration;
use crate::prelude::*;
use crate::registry::package::index::PackageIndex;
use crate::registry::signer::SignerInfo;
use crate::rpc_continuations::Guid;

pub struct RegisteredPackageSignatures;
impl RegistryMigration for RegisteredPackageSignatures {
    fn action(&self, db: &mut Value, _: &Path) -> Result<(), Error> {
        let signers: BTreeMap<Guid, SignerInfo> = from_value(db["index"]["signers"].clone())?;
        let mut packages: PackageIndex = from_value(db["index"]["package"].clone())?;
        packages.retain_registered_signatures(&signers);
        db["index"]["package"] = to_value(&packages)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::registry::package::index::PackageVersionInfo;
    use crate::registry::package::index::tests::{signers_with, version_info_signed_by};

    #[test]
    fn drops_signatures_from_unregistered_keys() {
        let (publisher, mut info) = version_info_signed_by(1);
        let (ci, ci_signed) = version_info_signed_by(2);
        info.merge_with(ci_signed, true).unwrap();
        let mut db = imbl_value::json!({
            "index": {
                "signers": to_value(&signers_with([publisher.clone()])).unwrap(),
                "package": {
                    "categories": {},
                    "packages": { "test": {
                        "authorized": {},
                        "categories": [],
                        "versions": { "1.0.0:0": to_value(&info).unwrap() },
                    } },
                },
            }
        });

        RegisteredPackageSignatures
            .action(&mut db, Path::new("/nonexistent"))
            .unwrap();

        let migrated: PackageVersionInfo =
            from_value(db["index"]["package"]["packages"]["test"]["versions"]["1.0.0:0"].clone())
                .unwrap();
        let kept = migrated.s9pks[0]
            .1
            .signatures
            .keys()
            .collect::<BTreeSet<_>>();
        assert_eq!(kept, [&publisher].into());
        assert!(!kept.contains(&ci));
    }
}
