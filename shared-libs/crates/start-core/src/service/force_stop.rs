use std::future::Future;
use std::time::Duration;

use crate::prelude::*;

pub(super) trait ForceStopTarget {
    fn kill_subcontainers(&mut self) -> impl Future<Output = Result<(), Error>> + Send;
    fn wait_stopped(&mut self) -> impl Future<Output = Result<(), Error>> + Send;
    fn drain_effects(&mut self) -> impl Future<Output = Result<(), Error>> + Send;
    fn kill_container(&mut self) -> impl Future<Output = Result<(), Error>> + Send;
    fn replace_stopped(&mut self) -> impl Future<Output = Result<(), Error>> + Send;
}

pub(super) async fn force_stop(target: &mut impl ForceStopTarget) -> Result<(), Error> {
    let graceful = async {
        target.kill_subcontainers().await?;
        target.wait_stopped().await
    };
    if tokio::time::timeout(Duration::from_secs(5), graceful)
        .await
        .is_ok_and(|res| res.is_ok())
    {
        return Ok(());
    }
    target.drain_effects().await?;
    target.kill_container().await?;
    target.replace_stopped().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Target {
        events: Vec<&'static str>,
        stalled: bool,
        fail_at: Option<&'static str>,
    }
    impl Target {
        fn record(&mut self, event: &'static str) -> Result<(), Error> {
            self.events.push(event);
            if self.fail_at == Some(event) {
                Err(Error::new(eyre!("{event}"), ErrorKind::Unknown))
            } else {
                Ok(())
            }
        }
    }
    impl ForceStopTarget for Target {
        async fn kill_subcontainers(&mut self) -> Result<(), Error> {
            self.record("subcontainers")
        }
        async fn wait_stopped(&mut self) -> Result<(), Error> {
            self.record("stop")?;
            if self.stalled {
                std::future::pending::<()>().await;
            }
            Ok(())
        }
        async fn drain_effects(&mut self) -> Result<(), Error> {
            self.record("drain")
        }
        async fn kill_container(&mut self) -> Result<(), Error> {
            self.record("kill")
        }
        async fn replace_stopped(&mut self) -> Result<(), Error> {
            self.record("replace")
        }
    }

    #[tokio::test(start_paused = true)]
    async fn force_stop_responsive_runtime_needs_no_replacement() {
        let mut target = Target::default();
        force_stop(&mut target).await.unwrap();
        assert_eq!(target.events, ["subcontainers", "stop"]);
    }

    #[tokio::test(start_paused = true)]
    async fn force_stop_stalled_runtime_recovers_after_five_seconds() {
        let mut target = Target {
            stalled: true,
            ..Default::default()
        };
        let start = tokio::time::Instant::now();
        force_stop(&mut target).await.unwrap();
        assert_eq!(start.elapsed(), Duration::from_secs(5));
        assert_eq!(
            target.events,
            ["subcontainers", "stop", "drain", "kill", "replace"]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn force_stop_failed_kill_never_replaces() {
        let mut target = Target {
            stalled: true,
            fail_at: Some("kill"),
            ..Default::default()
        };
        assert!(force_stop(&mut target).await.is_err());
        assert_eq!(target.events, ["subcontainers", "stop", "drain", "kill"]);
    }

    #[tokio::test(start_paused = true)]
    async fn force_stop_subcontainer_failure_recovers_without_waiting() {
        let mut target = Target {
            fail_at: Some("subcontainers"),
            ..Default::default()
        };
        force_stop(&mut target).await.unwrap();
        assert_eq!(target.events, ["subcontainers", "drain", "kill", "replace"]);
    }
}
