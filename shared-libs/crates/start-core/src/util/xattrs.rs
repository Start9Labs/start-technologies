use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io;
use std::path::Path;

use xattr::FileExt;

#[derive(Clone, Copy)]
pub(crate) enum Target<'a> {
    File(&'a File),
    NoFollowPath(&'a Path),
}

impl Target<'_> {
    fn names(self) -> io::Result<Vec<OsString>> {
        let names = match self {
            Self::File(file) => file.list_xattr(),
            Self::NoFollowPath(path) => xattr::list(path),
        };
        match names {
            Ok(names) => Ok(names.collect()),
            Err(error) if error.raw_os_error() == Some(libc::EOPNOTSUPP) => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }

    fn get(self, name: &OsStr) -> io::Result<Option<Vec<u8>>> {
        match self {
            Self::File(file) => file.get_xattr(name),
            Self::NoFollowPath(path) => xattr::get(path, name),
        }
    }

    fn set(self, name: &OsStr, value: &[u8]) -> io::Result<()> {
        match self {
            Self::File(file) => file.set_xattr(name, value),
            Self::NoFollowPath(path) => xattr::set(path, name, value),
        }
    }

    fn remove(self, name: &OsStr) -> io::Result<()> {
        match self {
            Self::File(file) => file.remove_xattr(name),
            Self::NoFollowPath(path) => xattr::remove(path, name),
        }
    }

    /// Removes inherited attributes absent from the source, including access ACLs.
    pub(crate) fn replace(self, values: &BTreeMap<OsString, Vec<u8>>) -> io::Result<()> {
        for name in self.names()? {
            if !values.contains_key(&name) {
                self.remove(&name)?;
            }
        }
        for (name, value) in values {
            self.set(name, value)?;
        }
        Ok(())
    }

    pub(crate) fn copy_to(self, destination: Self) -> io::Result<()> {
        let values = self
            .names()?
            .into_iter()
            .map(|name| {
                let value = self
                    .get(&name)?
                    .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
                Ok((name, value))
            })
            .collect::<io::Result<BTreeMap<_, _>>>()?;
        destination.replace(&values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_native_targets_replace_attributes_instead_of_merging() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("destination");
        std::fs::write(&source, b"source").unwrap();
        std::fs::write(&destination, b"destination").unwrap();
        xattr::set(&source, "user.source", b"retained").unwrap();
        for target in [
            Target::NoFollowPath(&destination),
            Target::File(&File::open(&destination).unwrap()),
        ] {
            xattr::set(&destination, "user.unexpected", b"remove-me").unwrap();
            Target::NoFollowPath(&source).copy_to(target).unwrap();
            assert_eq!(
                xattr::get(&destination, "user.source").unwrap().unwrap(),
                b"retained"
            );
            assert_eq!(xattr::get(&destination, "user.unexpected").unwrap(), None);
        }
    }

    #[test]
    fn path_copy_does_not_change_symlink_target_attributes() {
        let root = tempfile::tempdir().unwrap();
        let protected = root.path().join("protected");
        let source = root.path().join("source-link");
        let destination = root.path().join("destination-link");
        std::fs::write(&protected, b"protected").unwrap();
        std::os::unix::fs::symlink(&protected, &source).unwrap();
        std::os::unix::fs::symlink(&protected, &destination).unwrap();
        xattr::set(&protected, "user.retained", b"not-a-link-attribute").unwrap();
        Target::NoFollowPath(&source)
            .copy_to(Target::NoFollowPath(&destination))
            .unwrap();
        assert_eq!(
            xattr::get(&protected, "user.retained").unwrap().unwrap(),
            b"not-a-link-attribute"
        );
    }
}
