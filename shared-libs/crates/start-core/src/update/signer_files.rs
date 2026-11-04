use std::collections::BTreeSet;
use std::panic::UnwindSafe;
use std::path::{Path, PathBuf};

use tokio::sync::Mutex;

use crate::db::model::Database;
use crate::prelude::*;
use crate::registry::signer::AcceptSigners;
use crate::sign::AnyVerifyingKey;
use crate::trusted_keys::{TrustContext, enabled_keys};
use crate::util::io::{delete_file, read_file_to_string, write_file_atomic};

const SIGNERS: &str = "/etc/startos/os-signers";
const PERSISTENT_SIGNERS: &str = "/media/startos/config/overlay/etc/startos/os-signers";
static WRITE_LOCK: Mutex<()> = Mutex::const_new(());

async fn read_signers(directory: &Path) -> Result<BTreeSet<AnyVerifyingKey>, Error> {
    let mut files = tokio::fs::read_dir(directory)
        .await
        .with_kind(ErrorKind::Filesystem)?;
    let mut keys = BTreeSet::new();
    while let Some(file) = files.next_entry().await.with_kind(ErrorKind::Filesystem)? {
        if file.path().extension().is_some_and(|ext| ext == "pem") {
            keys.insert(read_file_to_string(file.path()).await?.parse()?);
        }
    }
    Ok(keys)
}

async fn read_policy(defaults: &Path, persistent: &Path) -> Result<AcceptSigners, Error> {
    // Overlay directory merging must not restore removed default signers.
    let directory = if tokio::fs::try_exists(persistent).await? {
        persistent
    } else {
        defaults
    };
    Ok(AcceptSigners::Any(
        read_signers(directory)
            .await?
            .into_iter()
            .map(AcceptSigners::Signer)
            .collect(),
    ))
}

pub(super) async fn policy() -> Result<AcceptSigners, Error> {
    read_policy(Path::new(SIGNERS), Path::new(PERSISTENT_SIGNERS)).await
}

async fn write_signers(directory: &Path, keys: &BTreeSet<AnyVerifyingKey>) -> Result<(), Error> {
    tokio::fs::create_dir_all(directory)
        .await
        .with_kind(ErrorKind::Filesystem)?;
    let mut names = BTreeSet::new();
    for key in keys {
        let filename = format!(
            "{}.pem",
            hex::encode(blake3::hash(key.interned_der64().as_bytes()).as_bytes())
        );
        write_file_atomic(directory.join(&filename), key.to_string()).await?;
        names.insert(filename);
    }
    let mut files = tokio::fs::read_dir(directory)
        .await
        .with_kind(ErrorKind::Filesystem)?;
    while let Some(file) = files.next_entry().await.with_kind(ErrorKind::Filesystem)? {
        if file.path().extension().is_some_and(|ext| ext == "pem")
            && !names.contains(&file.file_name().to_string_lossy().into_owned())
        {
            delete_file(file.path()).await?;
        }
    }
    tokio::fs::File::open(directory).await?.sync_all().await?;
    if let Some(parent) = directory.parent() {
        tokio::fs::File::open(parent).await?.sync_all().await?;
    }
    Ok(())
}

async fn sync(db: &TypedPatchDb<Database>, directory: &Path) -> Result<(), Error> {
    let keys = enabled_keys(
        &db.peek()
            .await
            .as_public()
            .as_server_info()
            .as_trusted_keys()
            .de()?,
        TrustContext::Os,
    )
    .into_iter()
    .collect();
    if !tokio::fs::try_exists(directory).await?
        || read_signers(directory)
            .await
            .map_or(true, |saved| saved != keys)
    {
        write_signers(directory, &keys).await?;
    }
    Ok(())
}

pub(crate) async fn initialize(db: &TypedPatchDb<Database>) -> Result<(), Error> {
    let _guard = WRITE_LOCK.lock().await;
    sync(db, Path::new(PERSISTENT_SIGNERS)).await
}

pub(crate) async fn mutate<T: UnwindSafe + Send + 'static>(
    db: TypedPatchDb<Database>,
    mutate: impl FnOnce(&mut Model<Database>) -> Result<T, Error> + UnwindSafe + Send + 'static,
) -> Result<T, Error> {
    mutate_and_sync(db, PathBuf::from(PERSISTENT_SIGNERS), mutate).await
}

async fn mutate_and_sync<T: UnwindSafe + Send + 'static>(
    db: TypedPatchDb<Database>,
    directory: PathBuf,
    mutate: impl FnOnce(&mut Model<Database>) -> Result<T, Error> + UnwindSafe + Send + 'static,
) -> Result<T, Error> {
    tokio::spawn(async move {
        let _guard = WRITE_LOCK.lock().await;
        let result = db.mutate(mutate).await.result?;
        sync(&db, &directory).await?;
        Ok(result)
    })
    .await
    .with_kind(ErrorKind::Unknown)?
}

#[cfg(test)]
mod tests {
    use patch_db::ModelExt;

    use super::*;
    use crate::util::io::TmpDir;

    #[tokio::test]
    async fn signer_files_replace_removed_keys_and_preserve_an_empty_selection() {
        let temp = TmpDir::new().await.unwrap();
        let keys = crate::update::signers::initial_signers()
            .into_keys()
            .map(|key| key.parse().unwrap())
            .collect::<BTreeSet<_>>();
        write_signers(&temp, &keys).await.unwrap();
        assert_eq!(read_signers(&temp).await.unwrap(), keys);
        let selected = keys.into_iter().take(1).collect();
        write_signers(&temp, &selected).await.unwrap();
        assert_eq!(read_signers(&temp).await.unwrap(), selected);
        write_signers(&temp, &BTreeSet::new()).await.unwrap();
        assert!(read_signers(&temp).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn recovery_policy_uses_the_saved_selection_instead_of_merging_defaults() {
        let temp = TmpDir::new().await.unwrap();
        let defaults = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../projects/start-os/build/etc/startos/os-signers");
        let seeded = read_signers(&defaults).await.unwrap();
        assert_eq!(
            seeded,
            crate::update::signers::initial_signers()
                .into_keys()
                .map(|key| key.parse().unwrap())
                .collect()
        );
        let saved = temp.join("saved");
        assert!(
            matches!(read_policy(&defaults, &saved).await.unwrap(), AcceptSigners::Any(keys) if keys.len() == seeded.len())
        );
        let custom = crate::sign::AnyVerifyingKey::Ed25519(
            ed25519_dalek::SigningKey::from_bytes(&[42; 32]).verifying_key(),
        );
        write_signers(&saved, &BTreeSet::from([custom.clone()]))
            .await
            .unwrap();
        assert!(
            matches!(read_policy(&defaults, &saved).await.unwrap(), AcceptSigners::Any(keys) if matches!(keys.as_slice(), [AcceptSigners::Signer(key)] if key == &custom))
        );
        write_signers(&saved, &BTreeSet::new()).await.unwrap();
        assert!(
            matches!(read_policy(&defaults, &saved).await.unwrap(), AcceptSigners::Any(keys) if keys.is_empty())
        );
    }

    #[tokio::test]
    async fn successful_trust_mutation_persists_os_keys_before_returning() {
        let temp = TmpDir::new().await.unwrap();
        let db = TypedPatchDb::<Database>::load_unchecked(patch_db::PatchDb::in_memory());
        let keys = crate::trusted_keys::initial_trusted_keys();
        db.put(
            &patch_db::json_ptr::ROOT,
            &imbl_value::json!({"public": {"serverInfo": {"trustedKeys": keys}}}),
        )
        .await
        .unwrap();
        let directory = temp.join("saved");
        let selected = keys.values().next().unwrap().public_key.clone();
        let expected = BTreeSet::from([selected.clone()]);
        mutate_and_sync(db.clone(), directory.clone(), move |db| {
            let mut keys = db.as_public().as_server_info().as_trusted_keys().de()?;
            for key in keys.values_mut() {
                key.os = key.public_key == selected;
            }
            db.as_public_mut()
                .as_server_info_mut()
                .as_trusted_keys_mut()
                .ser(&keys)
        })
        .await
        .unwrap();
        assert_eq!(read_signers(&directory).await.unwrap(), expected);
        let before = db.peek().await.into_value();
        let result = mutate_and_sync(db.clone(), directory.clone(), |_| -> Result<(), Error> {
            Err(Error::new(eyre!("rejected"), ErrorKind::InvalidRequest))
        })
        .await;
        assert!(result.is_err());
        assert_eq!(db.peek().await.into_value(), before);
        assert_eq!(read_signers(&directory).await.unwrap(), expected);
        write_file_atomic(directory.join("broken.pem"), "broken")
            .await
            .unwrap();
        sync(&db, &directory).await.unwrap();
        assert_eq!(read_signers(&directory).await.unwrap(), expected);
    }

    #[tokio::test]
    async fn startup_sync_repairs_missing_and_stale_recovery_signers() {
        let temp = TmpDir::new().await.unwrap();
        let db = TypedPatchDb::<Database>::load_unchecked(patch_db::PatchDb::in_memory());
        let keys = crate::trusted_keys::initial_trusted_keys();
        db.put(
            &patch_db::json_ptr::ROOT,
            &imbl_value::json!({"public": {"serverInfo": {"trustedKeys": keys}}}),
        )
        .await
        .unwrap();
        let directory = temp.join("saved");
        sync(&db, &directory).await.unwrap();
        assert_eq!(read_signers(&directory).await.unwrap().len(), 3);
        let selected = keys.values().next().unwrap().public_key.clone();
        let expected = BTreeSet::from([selected.clone()]);
        db.mutate(move |db| {
            let mut keys = db.as_public().as_server_info().as_trusted_keys().de()?;
            for key in keys.values_mut() {
                key.os = key.public_key == selected;
            }
            db.as_public_mut()
                .as_server_info_mut()
                .as_trusted_keys_mut()
                .ser(&keys)
        })
        .await
        .result
        .unwrap();
        assert_eq!(read_signers(&directory).await.unwrap().len(), 3);
        sync(&db, &directory).await.unwrap();
        assert_eq!(read_signers(&directory).await.unwrap(), expected);
        crate::util::io::delete_dir(&directory).await.unwrap();
        sync(&db, &directory).await.unwrap();
        assert_eq!(read_signers(&directory).await.unwrap(), expected);
    }

    #[tokio::test]
    async fn malformed_signer_files_fail_closed() {
        let temp = TmpDir::new().await.unwrap();
        write_file_atomic(temp.join("invalid.pem"), "not a key")
            .await
            .unwrap();
        assert!(read_signers(&temp).await.is_err());
    }
}
