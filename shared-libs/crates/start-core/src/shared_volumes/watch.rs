use std::collections::BTreeMap;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::Instant;

use super::{SharedVolume, SharedVolumeAvailability, SharedVolumeId, SharedVolumeSource, source};
use crate::context::RpcContext;
use crate::prelude::*;
use crate::util::future::NonDetachingJoinHandle;

mod events;

#[derive(Debug)]
enum Event {
    Disks,
    Network,
}

struct Retry {
    next: Instant,
    delay: Duration,
}

impl Retry {
    fn new(now: Instant) -> Self {
        Self {
            next: now,
            delay: Duration::from_secs(2),
        }
    }

    fn take(&mut self, now: Instant) -> bool {
        if now < self.next {
            return false;
        }
        self.next = now + self.delay;
        self.delay = (self.delay * 2).min(Duration::from_secs(300));
        true
    }
}

fn retryable(volume: &SharedVolume) -> bool {
    matches!(volume.source, SharedVolumeSource::Cifs { .. })
        && volume.availability == SharedVolumeAvailability::Unavailable
        && !volume.grants.is_empty()
}

async fn retry(ctx: RpcContext, id: SharedVolumeId) -> Result<(), Error> {
    let volumes = ctx.db.peek().await.as_public().as_shared_volumes().de()?;
    let Some(SharedVolume {
        source: SharedVolumeSource::Cifs { hostname, .. },
        ..
    }) = volumes.get(&id)
    else {
        return Ok(());
    };
    if crate::disk::mount::filesystem::cifs::reachable(hostname).await {
        super::inspect(ctx, super::SharedVolumeParams { id }).await?;
    }
    Ok(())
}

pub(crate) async fn run(context: impl Fn() -> Option<RpcContext>) {
    if let Err(error) = watch(context).await {
        tracing::error!(
            "{}",
            t!("shared-volumes.availability-watch-failed", error = error)
        );
    }
}

async fn watch(context: impl Fn() -> Option<RpcContext>) -> Result<(), Error> {
    let Some(ctx) = context() else { return Ok(()) };
    let db = ctx.db.clone();
    drop(ctx);
    let (send, mut recv) = mpsc::channel(64);
    let _listeners = events::subscribe(send);
    let mut catalog = db.watch("/public/sharedVolumes".parse().unwrap()).await;
    let mut running: BTreeMap<SharedVolumeId, NonDetachingJoinHandle<()>> = BTreeMap::new();
    let mut retries = BTreeMap::<SharedVolumeId, Retry>::new();
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    let mut startup = true;
    loop {
        let event = if startup {
            None
        } else {
            tokio::select! {
                event = recv.recv() => event,
                changed = catalog.changed() => { changed?; None },
                _ = tick.tick() => None,
            }
        };
        let Some(ctx) = context() else { break };
        let volumes: BTreeMap<SharedVolumeId, SharedVolume> =
            db.peek().await.as_public().as_shared_volumes().de()?;
        running.retain(|id, job| volumes.contains_key(id) && !job.is_finished());
        retries.retain(|id, _| volumes.get(id).is_some_and(retryable));
        if matches!(event, Some(Event::Disks)) {
            ctx.db
                .mutate(|db| {
                    let status = db.as_public_mut().as_server_info_mut().as_status_info_mut();
                    let revision = status.as_disk_revision().de()?;
                    status.as_disk_revision_mut().ser(&(revision + 1))
                })
                .await
                .result?;
        }
        for (id, volume) in volumes {
            let event_matches = match (&event, &volume.source) {
                (Some(Event::Disks), SharedVolumeSource::Disk { .. }) => true,
                (Some(Event::Network), SharedVolumeSource::Cifs { .. }) => true,
                _ => false,
            };
            let full = startup
                || event_matches
                || volume.availability == SharedVolumeAvailability::Unknown;
            if event_matches {
                running.remove(&id);
            }
            if full {
                if let SharedVolumeSource::Disk { logicalname, .. } = &volume.source {
                    if !tokio::fs::try_exists(logicalname).await? {
                        running.remove(&id);
                        source::record_availability(
                            &ctx,
                            &id,
                            SharedVolumeAvailability::Unavailable,
                        )
                        .await?;
                        continue;
                    }
                }
            }
            if running.contains_key(&id) {
                continue;
            }
            let tcp = retryable(&volume)
                && retries
                    .entry(id.clone())
                    .or_insert_with(|| Retry::new(Instant::now()))
                    .take(Instant::now());
            if full || tcp {
                let ctx = ctx.clone();
                let key = id.clone();
                running.insert(
                    id,
                    tokio::spawn(async move {
                        if full {
                            super::inspect(ctx, super::SharedVolumeParams { id: key })
                                .await
                                .log_err();
                        } else {
                            retry(ctx, key).await.log_err();
                        }
                    })
                    .into(),
                );
            }
        }
        startup = false;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_back_off_to_five_minutes() {
        let now = Instant::now();
        let mut retry = Retry::new(now);
        assert!(retry.take(now));
        assert!(!retry.take(now + Duration::from_secs(1)));
        assert!(retry.take(now + Duration::from_secs(2)));
        assert_eq!(retry.next, now + Duration::from_secs(6));
        for _ in 0..20 {
            assert!(retry.take(retry.next));
        }
        assert_eq!(retry.delay, Duration::from_secs(300));
        let next = retry.next;
        assert!(retry.take(next));
        assert_eq!(retry.next - next, Duration::from_secs(300));
    }

    #[test]
    fn retry_requires_an_unavailable_granted_cifs_share() {
        let mut volume = SharedVolume {
            name: "NAS".into(),
            source: SharedVolumeSource::Cifs {
                hostname: "nas.local".into(),
                path: "/share".into(),
            },
            grants: BTreeMap::new(),
            availability: SharedVolumeAvailability::Unavailable,
        };
        assert!(!retryable(&volume));
        volume.grants.insert(
            "consumer".parse().unwrap(),
            super::super::SharedVolumeGrant { readonly: true },
        );
        assert!(retryable(&volume));
        volume.availability = SharedVolumeAvailability::Available;
        assert!(!retryable(&volume));
        volume.availability = SharedVolumeAvailability::Unknown;
        assert!(!retryable(&volume));
        volume.availability = SharedVolumeAvailability::Unavailable;
        volume.source = SharedVolumeSource::Local;
        assert!(!retryable(&volume));
    }
}
