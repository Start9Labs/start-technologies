use std::io::Write;
use std::path::{Path, PathBuf};

use chacha20::Key;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::atomic_file::AtomicFile;
use crate::error::{BkfsError, BkfsResult};
use crate::serde::{data_config, decode, encode};
use crate::vault::{self, AuthenticationTag, Binding, EccParams};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct Reference {
    tag: AuthenticationTag,
}

impl Reference {
    /// The root publisher must sync the containing directory before referencing this blob.
    pub fn write<T: Serialize + DeserializeOwned>(
        segments: &Path,
        key: &Key,
        ecc: EccParams,
        binding: Binding,
        value: &T,
    ) -> BkfsResult<Self> {
        let plain = encode(value, data_config())?;
        let _: T = decode(&plain, data_config())?;
        let sealed = vault::seal_bound(&plain, key, ecc, binding);
        let reference = Self { tag: sealed.tag };
        let path = reference.path(segments);
        std::fs::create_dir_all(path.parent().unwrap())?;
        let mut file = AtomicFile::create_buffered(path.clone())?;
        file.write_all(&sealed.bytes)?;
        file.save()?;
        Ok(reference)
    }

    pub fn load<T: DeserializeOwned>(
        self,
        segments: &Path,
        key: &Key,
        binding: Binding,
    ) -> BkfsResult<T> {
        let blob = std::fs::read(self.path(segments)).map_err(BkfsError::referenced_io)?;
        let plain = vault::open_bound(&blob, key, binding)?.verify_reference(&self.tag)?;
        decode(&plain, data_config())
    }

    pub fn directory(segments: &Path) -> PathBuf {
        segments.join("state-objects")
    }

    pub fn path(self, segments: &Path) -> PathBuf {
        Self::directory(segments).join(crate::util::hex(&self.tag))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_rejects_a_valid_older_blob_at_the_current_path() {
        let dir = tempfile::tempdir().unwrap();
        let key = Key::from([0x72; 32]);
        let older = Reference::write(
            dir.path(),
            &key,
            EccParams::default(),
            Binding::Transaction,
            &"older".to_owned(),
        )
        .unwrap();
        let current = Reference::write(
            dir.path(),
            &key,
            EccParams::default(),
            Binding::Transaction,
            &"current".to_owned(),
        )
        .unwrap();
        assert_ne!(older, current);
        assert_eq!(
            current
                .load::<String>(dir.path(), &key, Binding::Transaction)
                .unwrap(),
            "current"
        );
        std::fs::copy(older.path(dir.path()), current.path(dir.path())).unwrap();
        assert!(current
            .load::<String>(dir.path(), &key, Binding::Transaction)
            .is_err());
        assert_eq!(
            older
                .load::<String>(dir.path(), &key, Binding::Transaction)
                .unwrap(),
            "older"
        );
    }

    #[test]
    fn reference_rejects_another_metadata_role() {
        let dir = tempfile::tempdir().unwrap();
        let key = Key::from([0x73; 32]);
        let reference = Reference::write(
            dir.path(),
            &key,
            EccParams::default(),
            Binding::Catalog,
            &vec![1u8, 2, 3],
        )
        .unwrap();
        assert!(reference
            .load::<Vec<u8>>(dir.path(), &key, Binding::Transaction)
            .is_err());
        assert_eq!(
            reference
                .load::<Vec<u8>>(dir.path(), &key, Binding::Catalog)
                .unwrap(),
            [1, 2, 3]
        );
    }

    #[test]
    fn reference_keeps_its_identity_after_recoverable_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let key = Key::from([0x74; 32]);
        let reference = Reference::write(
            dir.path(),
            &key,
            EccParams::default(),
            Binding::Transaction,
            &vec![7u8; 1000],
        )
        .unwrap();
        let path = reference.path(dir.path());
        let mut blob = std::fs::read(&path).unwrap();
        let middle = blob.len() / 2;
        blob[middle] ^= 1;
        std::fs::write(path, blob).unwrap();
        assert_eq!(
            reference
                .load::<Vec<u8>>(dir.path(), &key, Binding::Transaction)
                .unwrap(),
            vec![7; 1000]
        );
    }
}
