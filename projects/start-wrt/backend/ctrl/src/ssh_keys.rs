use std::path::Path;

use clap::Parser;
use rpc_toolkit::{from_fn_async, Context, HandlerExt, ParentHandler};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tracing::instrument;

use crate::error::{Error, ErrorKind};
use crate::prelude::*;
use crate::utils::HandlerExtSerde;
use crate::{CliContext, ServerContext};

const SSH_DIR: &str = "/etc/dropbear";
const AUTHORIZED_KEYS: &str = "/etc/dropbear/authorized_keys";

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshKeyResponse {
    pub algorithm: String,
    pub fingerprint: String,
    pub hostname: String,
}

pub fn ssh_keys<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "list",
            from_fn_async(list)
                .with_display_serializable()
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "add",
            from_fn_async(add)
                .with_display_serializable()
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "delete",
            from_fn_async(delete)
                .no_display()
                .with_call_remote::<CliContext>(),
        )
}

#[instrument(skip_all)]
pub async fn list(_ctx: ServerContext) -> Result<Vec<SshKeyResponse>, Error> {
    list_keys(Path::new(AUTHORIZED_KEYS)).await
}

#[derive(Deserialize, Serialize, Parser)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct SshKeyAddParams {
    key: String,
}

#[instrument(skip_all)]
pub async fn add(
    _ctx: ServerContext,
    SshKeyAddParams { key }: SshKeyAddParams,
) -> Result<SshKeyResponse, Error> {
    let result = add_key(Path::new(SSH_DIR), Path::new(AUTHORIZED_KEYS), &key).await;
    match &result {
        Ok(resp) => crate::activity::log(
            "ssh-key", "added", true,
            &format!("Added SSH key ({}: {})", resp.algorithm, resp.fingerprint), None,
        ),
        Err(e) => crate::activity::log(
            "ssh-key", "added", false,
            "Failed to add SSH key", Some(&e.to_string()),
        ),
    }
    result
}

#[derive(Deserialize, Serialize, Parser)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct SshKeyDeleteParams {
    fingerprint: String,
}

#[instrument(skip_all)]
pub async fn delete(
    _ctx: ServerContext,
    SshKeyDeleteParams { fingerprint }: SshKeyDeleteParams,
) -> Result<(), Error> {
    let result = delete_key(Path::new(AUTHORIZED_KEYS), &fingerprint).await;
    crate::activity::log_result("ssh-key", "deleted", &format!("SSH key ({fingerprint})"), result)
}

// --- Core logic (testable, accepts paths) ---

async fn list_keys(authorized_keys: &Path) -> Result<Vec<SshKeyResponse>, Error> {
    let contents = match tokio::fs::read_to_string(authorized_keys).await {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };

    let mut keys = Vec::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        match trimmed.parse::<openssh_keys::PublicKey>() {
            Ok(pk) => keys.push(SshKeyResponse {
                algorithm: pk.keytype().to_owned(),
                fingerprint: pk.fingerprint_md5(),
                hostname: pk.comment.clone().unwrap_or_default(),
            }),
            Err(_) => continue,
        }
    }

    Ok(keys)
}

async fn add_key(
    ssh_dir: &Path,
    authorized_keys: &Path,
    key: &str,
) -> Result<SshKeyResponse, Error> {
    let pk: openssh_keys::PublicKey = key
        .trim()
        .parse()
        .map_err(|e| Error::new(eyre!("invalid SSH public key: {e}"), ErrorKind::ParseSshKey))?;

    let fingerprint = pk.fingerprint_md5();

    // Check for duplicates
    let existing = match tokio::fs::read_to_string(authorized_keys).await {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };

    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Ok(existing_pk) = trimmed.parse::<openssh_keys::PublicKey>() {
            if existing_pk.fingerprint_md5() == fingerprint {
                return Err(Error::new(eyre!("SSH key already exists"), ErrorKind::Duplicate));
            }
        }
    }

    // Ensure ssh directory exists with correct permissions
    if tokio::fs::metadata(ssh_dir).await.is_err() {
        tokio::fs::create_dir_all(ssh_dir).await?;
        set_permissions(ssh_dir, 0o700).await?;
    }

    // Append key to authorized_keys
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(authorized_keys)
        .await?;
    file.write_all(pk.to_key_format().as_bytes()).await?;
    file.write_all(b"\n").await?;
    file.sync_all().await?;

    // Ensure correct permissions on authorized_keys
    set_permissions(authorized_keys, 0o600).await?;

    Ok(SshKeyResponse {
        algorithm: pk.keytype().to_owned(),
        fingerprint,
        hostname: pk.comment.unwrap_or_default(),
    })
}

async fn delete_key(authorized_keys: &Path, fingerprint: &str) -> Result<(), Error> {
    let contents = match tokio::fs::read_to_string(authorized_keys).await {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::new(eyre!("SSH key not found"), ErrorKind::NotFound));
        }
        Err(e) => return Err(e.into()),
    };

    let mut found = false;
    let mut remaining = Vec::new();

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            remaining.push(line.to_owned());
            continue;
        }
        match trimmed.parse::<openssh_keys::PublicKey>() {
            Ok(pk) if pk.fingerprint_md5() == fingerprint => {
                found = true;
            }
            _ => {
                remaining.push(line.to_owned());
            }
        }
    }

    if !found {
        return Err(Error::new(eyre!("SSH key not found"), ErrorKind::NotFound));
    }

    // Rewrite the file
    let mut file = tokio::fs::File::create(authorized_keys).await?;
    for line in &remaining {
        file.write_all(line.as_bytes()).await?;
        file.write_all(b"\n").await?;
    }
    file.sync_all().await?;

    Ok(())
}

async fn set_permissions(path: &Path, mode: u32) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(mode);
    tokio::fs::set_permissions(path, perms).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    const TEST_KEY_1: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl matt@macbook";
    const TEST_KEY_2: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJf3LQXK5m7dZtQgkVwMYxPragThKvOHPrLwfCfMR7fa lucy@desktop";

    fn fingerprint_of(key: &str) -> String {
        key.parse::<openssh_keys::PublicKey>()
            .unwrap()
            .fingerprint_md5()
    }

    // === list tests ===

    #[tokio::test]
    async fn test_list_empty_when_file_missing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("authorized_keys");

        let result = list_keys(&path).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_list_empty_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("authorized_keys");
        tokio::fs::write(&path, "").await.unwrap();

        let result = list_keys(&path).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_list_parses_keys() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("authorized_keys");
        tokio::fs::write(&path, format!("{TEST_KEY_1}\n{TEST_KEY_2}\n"))
            .await
            .unwrap();

        let result = list_keys(&path).await.unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].algorithm, "ssh-ed25519");
        assert_eq!(result[0].hostname, "matt@macbook");
        assert_eq!(result[1].hostname, "lucy@desktop");
    }

    #[tokio::test]
    async fn test_list_skips_comments_and_blank_lines() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("authorized_keys");
        let contents = format!("# a comment\n\n{TEST_KEY_1}\n  \n# another comment\n");
        tokio::fs::write(&path, contents).await.unwrap();

        let result = list_keys(&path).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].hostname, "matt@macbook");
    }

    #[tokio::test]
    async fn test_list_skips_unparseable_lines() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("authorized_keys");
        let contents = format!("not a valid key\n{TEST_KEY_1}\ngibberish stuff\n");
        tokio::fs::write(&path, contents).await.unwrap();

        let result = list_keys(&path).await.unwrap();
        assert_eq!(result.len(), 1);
    }

    // === add tests ===

    #[tokio::test]
    async fn test_add_creates_dir_and_file() {
        let dir = tempdir().unwrap();
        let ssh_dir = dir.path().join(".ssh");
        let auth_keys = ssh_dir.join("authorized_keys");

        let result = add_key(&ssh_dir, &auth_keys, TEST_KEY_1).await.unwrap();
        assert_eq!(result.algorithm, "ssh-ed25519");
        assert_eq!(result.hostname, "matt@macbook");
        assert!(!result.fingerprint.is_empty());

        // Verify file was created
        let contents = tokio::fs::read_to_string(&auth_keys).await.unwrap();
        assert!(contents.contains("ssh-ed25519"));

        // Verify directory permissions
        let dir_meta = tokio::fs::metadata(&ssh_dir).await.unwrap();
        assert_eq!(dir_meta.permissions().mode() & 0o777, 0o700);

        // Verify file permissions
        let file_meta = tokio::fs::metadata(&auth_keys).await.unwrap();
        assert_eq!(file_meta.permissions().mode() & 0o777, 0o600);
    }

    #[tokio::test]
    async fn test_add_appends_to_existing() {
        let dir = tempdir().unwrap();
        let ssh_dir = dir.path().join(".ssh");
        let auth_keys = ssh_dir.join("authorized_keys");

        add_key(&ssh_dir, &auth_keys, TEST_KEY_1).await.unwrap();
        add_key(&ssh_dir, &auth_keys, TEST_KEY_2).await.unwrap();

        let keys = list_keys(&auth_keys).await.unwrap();
        assert_eq!(keys.len(), 2);
    }

    #[tokio::test]
    async fn test_add_rejects_duplicate() {
        let dir = tempdir().unwrap();
        let ssh_dir = dir.path().join(".ssh");
        let auth_keys = ssh_dir.join("authorized_keys");

        add_key(&ssh_dir, &auth_keys, TEST_KEY_1).await.unwrap();
        let err = add_key(&ssh_dir, &auth_keys, TEST_KEY_1).await.unwrap_err();
        assert!(format!("{err}").contains("already exists"));
    }

    #[tokio::test]
    async fn test_add_rejects_invalid_key() {
        let dir = tempdir().unwrap();
        let ssh_dir = dir.path().join(".ssh");
        let auth_keys = ssh_dir.join("authorized_keys");

        let err = add_key(&ssh_dir, &auth_keys, "not-a-key").await.unwrap_err();
        assert!(format!("{err}").contains("invalid SSH public key"));
    }

    #[tokio::test]
    async fn test_add_returns_correct_fingerprint() {
        let dir = tempdir().unwrap();
        let ssh_dir = dir.path().join(".ssh");
        let auth_keys = ssh_dir.join("authorized_keys");

        let result = add_key(&ssh_dir, &auth_keys, TEST_KEY_1).await.unwrap();
        assert_eq!(result.fingerprint, fingerprint_of(TEST_KEY_1));
    }

    // === delete tests ===

    #[tokio::test]
    async fn test_delete_removes_key() {
        let dir = tempdir().unwrap();
        let ssh_dir = dir.path().join(".ssh");
        let auth_keys = ssh_dir.join("authorized_keys");

        add_key(&ssh_dir, &auth_keys, TEST_KEY_1).await.unwrap();
        add_key(&ssh_dir, &auth_keys, TEST_KEY_2).await.unwrap();

        let fp1 = fingerprint_of(TEST_KEY_1);
        delete_key(&auth_keys, &fp1).await.unwrap();

        let keys = list_keys(&auth_keys).await.unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].hostname, "lucy@desktop");
    }

    #[tokio::test]
    async fn test_delete_errors_when_not_found() {
        let dir = tempdir().unwrap();
        let ssh_dir = dir.path().join(".ssh");
        let auth_keys = ssh_dir.join("authorized_keys");

        add_key(&ssh_dir, &auth_keys, TEST_KEY_1).await.unwrap();

        let err = delete_key(&auth_keys, "nonexistent:fingerprint")
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("not found"));
    }

    #[tokio::test]
    async fn test_delete_errors_when_file_missing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("authorized_keys");

        let err = delete_key(&path, "any:fingerprint").await.unwrap_err();
        assert!(format!("{err}").contains("not found"));
    }

    #[tokio::test]
    async fn test_delete_preserves_comments() {
        let dir = tempdir().unwrap();
        let auth_keys = dir.path().join("authorized_keys");

        let contents = format!("# important comment\n{TEST_KEY_1}\n{TEST_KEY_2}\n");
        tokio::fs::write(&auth_keys, contents).await.unwrap();

        let fp1 = fingerprint_of(TEST_KEY_1);
        delete_key(&auth_keys, &fp1).await.unwrap();

        let file = tokio::fs::read_to_string(&auth_keys).await.unwrap();
        assert!(file.contains("# important comment"));

        let keys = list_keys(&auth_keys).await.unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].hostname, "lucy@desktop");
    }

    // === round-trip tests ===

    #[tokio::test]
    async fn test_add_then_list_then_delete_round_trip() {
        let dir = tempdir().unwrap();
        let ssh_dir = dir.path().join(".ssh");
        let auth_keys = ssh_dir.join("authorized_keys");

        // Start empty
        assert!(list_keys(&auth_keys).await.unwrap().is_empty());

        // Add two keys
        let r1 = add_key(&ssh_dir, &auth_keys, TEST_KEY_1).await.unwrap();
        let r2 = add_key(&ssh_dir, &auth_keys, TEST_KEY_2).await.unwrap();

        // List returns both
        let keys = list_keys(&auth_keys).await.unwrap();
        assert_eq!(keys.len(), 2);

        // Delete first by fingerprint from list
        delete_key(&auth_keys, &r1.fingerprint).await.unwrap();

        // Only second remains
        let keys = list_keys(&auth_keys).await.unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].fingerprint, r2.fingerprint);

        // Delete second
        delete_key(&auth_keys, &r2.fingerprint).await.unwrap();
        assert!(list_keys(&auth_keys).await.unwrap().is_empty());
    }
}
