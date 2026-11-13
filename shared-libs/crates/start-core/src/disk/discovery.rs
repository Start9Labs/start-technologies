use std::time::Duration;

use crate::db::model::Database;
use crate::disk::OsPartitionInfo;
use crate::prelude::*;

pub(crate) async fn refresh(db: TypedPatchDb<Database>, os: OsPartitionInfo) {
    loop {
        match scan(&db, &os).await {
            Ok(_) => break,
            Err(error) => tracing::warn!(
                "{}",
                t!("shared-volumes.availability-watch-failed", error = error)
            ),
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

pub(crate) async fn scan(
    db: &TypedPatchDb<Database>,
    os: &OsPartitionInfo,
) -> Result<Vec<super::util::DiskInfo>, Error> {
    let server_id = db.peek().await.as_public().as_server_info().as_id().de()?;
    let disks = super::util::list(os, Some(server_id.as_str())).await?;
    db.mutate(|db| db.as_public_mut().as_disks_mut().ser(&disks))
        .await
        .result?;
    Ok(disks)
}

pub(crate) async fn remove_missing(db: &TypedPatchDb<Database>) -> Result<(), Error> {
    let mut disks = db.peek().await.as_public().as_disks().de()?;
    let mut present = Vec::with_capacity(disks.len());
    for mut disk in disks.drain(..) {
        if !tokio::fs::try_exists(&disk.logicalname).await? {
            continue;
        }
        let mut partitions = Vec::with_capacity(disk.partitions.len());
        for partition in disk.partitions.drain(..) {
            if tokio::fs::try_exists(&partition.logicalname).await? {
                partitions.push(partition);
            }
        }
        disk.partitions = partitions;
        present.push(disk);
    }
    db.mutate(|db| db.as_public_mut().as_disks_mut().ser(&present))
        .await
        .result
}
