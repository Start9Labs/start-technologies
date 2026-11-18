use std::sync::Arc;

use crate::lxc::LxcControl;
use crate::prelude::*;
use crate::util::future::TaskControl;

/// Revocation retires the LXC payload and its host-side handlers together.
/// Handles delegated outside the incarnation remain outside this boundary.
#[derive(Clone)]
pub(crate) struct ServiceRuntime {
    container: Arc<LxcControl>,
    actor: TaskControl,
    effects: TaskControl,
}

impl ServiceRuntime {
    pub(crate) fn new(
        container: Arc<LxcControl>,
        actor: TaskControl,
        effects: TaskControl,
    ) -> Self {
        Self {
            container,
            actor,
            effects,
        }
    }

    pub(crate) fn payload_is_stopped(&self) -> bool {
        matches!(self.container.is_stopped(), Ok(true))
    }

    pub(crate) fn is_retired(&self) -> bool {
        self.payload_is_stopped() && self.actor.is_finished() && self.effects.is_finished()
    }

    pub(crate) async fn revoke(&self) -> Result<(), Error> {
        self.container.kill_and_wait().await?;
        let (actor, effects) =
            tokio::join!(self.actor.cancel_and_wait(), self.effects.cancel_and_wait());
        for result in [actor, effects] {
            if let Err(error) = result {
                if !error.is_cancelled() {
                    tracing::error!("Host task failed during service retirement: {error}");
                }
            }
        }
        self.container.kill_and_wait().await
    }
}
