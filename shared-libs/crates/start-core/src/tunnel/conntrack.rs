use std::path::Path;

use crate::prelude::*;
use crate::system::{MebiBytes, get_mem_info};
use crate::util::io::read_file_to_string;

pub(super) async fn configure() -> Result<(), Error> {
    configure_at(
        Path::new("/proc/sys/net/netfilter"),
        get_mem_info().await?.total,
    )
    .await
}

async fn configure_at(directory: &Path, memory: MebiBytes) -> Result<(), Error> {
    let max_path = directory.join("nf_conntrack_max");
    let target =
        ((memory.0 * 256.0).min(i32::MAX as f64) as u32).max(read_setting(&max_path).await?);
    raise_setting(&directory.join("nf_conntrack_buckets"), target).await?;
    raise_setting(&max_path, target).await
}

async fn read_setting(path: &Path) -> Result<u32, Error> {
    read_file_to_string(path)
        .await?
        .trim()
        .parse::<u32>()
        .with_ctx(|_| (ErrorKind::Network, path.display()))
}

async fn raise_setting(path: &Path, minimum: u32) -> Result<(), Error> {
    if read_setting(path).await? < minimum {
        tokio::fs::write(path, format!("{minimum}\n"))
            .await
            .with_ctx(|_| (ErrorKind::Network, path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn capacity_scales_with_ram_and_preserves_larger_settings() {
        for (memory, max, buckets, expected_max, expected_buckets) in [
            (464.0, 4096, 4096, 118784, 118784),
            (512.0, 4096, 4096, 131072, 131072),
            (1024.0, 8192, 8192, 262144, 262144),
            (4096.0, 65536, 65536, 1048576, 1048576),
            (512.0, 262144, 4096, 262144, 262144),
            (512.0, 4096, 262144, 131072, 262144),
            (512.0, 262144, 524288, 262144, 524288),
            (8388608.0, 262144, 262144, 2147483647, 2147483647),
        ] {
            let directory = tempfile::tempdir().unwrap();
            let max_path = directory.path().join("nf_conntrack_max");
            let buckets_path = directory.path().join("nf_conntrack_buckets");
            tokio::fs::write(&max_path, format!("{max}\n"))
                .await
                .unwrap();
            tokio::fs::write(&buckets_path, format!("{buckets}\n"))
                .await
                .unwrap();

            for _ in 0..2 {
                configure_at(directory.path(), MebiBytes(memory))
                    .await
                    .unwrap();
                assert_eq!(read_setting(&max_path).await.unwrap(), expected_max);
                assert_eq!(read_setting(&buckets_path).await.unwrap(), expected_buckets);
            }
        }
    }

    #[tokio::test]
    async fn bucket_failure_leaves_the_entry_limit_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let max_path = directory.path().join("nf_conntrack_max");
        tokio::fs::write(&max_path, "4096\n").await.unwrap();

        assert!(
            configure_at(directory.path(), MebiBytes(512.0))
                .await
                .is_err()
        );
        assert_eq!(read_setting(&max_path).await.unwrap(), 4096);
    }
}
