use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::context::RpcContext;
use crate::context::rpc::WeakRpcContext;
use crate::middleware::auth::signature::SignatureAuthorizationContext;
use crate::prelude::*;
use crate::sign::AnyVerifyingKey;

/// How long a passkey session key outlives its last connection.
pub const GRACE: Duration = Duration::from_secs(600);

struct Tracked {
    passkey: InternedString,
    connections: usize,
    idle_since: Option<Instant>,
}

/// Session keys passkeys enrolled, alive while connected and for [`GRACE`] after.
#[derive(Default)]
pub struct PasskeySessions(BTreeMap<InternedString, Tracked>);

impl PasskeySessions {
    pub fn enroll(&mut self, key: InternedString, passkey: InternedString, now: Instant) {
        self.0.insert(
            key,
            Tracked {
                passkey,
                connections: 0,
                idle_since: Some(now),
            },
        );
    }

    /// Whether the key is a passkey session key.
    pub fn connect(&mut self, key: &InternedString) -> bool {
        let Some(tracked) = self.0.get_mut(key) else {
            return false;
        };
        tracked.connections += 1;
        tracked.idle_since = None;
        true
    }

    pub fn disconnect(&mut self, key: &InternedString, now: Instant) {
        if let Some(tracked) = self.0.get_mut(key) {
            tracked.connections = tracked.connections.saturating_sub(1);
            if tracked.connections == 0 {
                tracked.idle_since = Some(now);
            }
        }
    }

    /// Forgets and reports a key that has had no connection for [`GRACE`].
    pub fn take_expired(&mut self, key: &InternedString, now: Instant) -> bool {
        let expired = self.0.get(key).is_some_and(|tracked| {
            tracked
                .idle_since
                .is_some_and(|since| now.saturating_duration_since(since) >= GRACE)
        });
        if expired {
            self.0.remove(key);
        }
        expired
    }

    /// Forgets and returns the session keys the passkey enrolled.
    pub fn revoke_passkey(&mut self, passkey: &InternedString) -> Vec<InternedString> {
        let keys: Vec<_> = self
            .0
            .iter()
            .filter(|(_, tracked)| tracked.passkey == *passkey)
            .map(|(key, _)| key.clone())
            .collect();
        for key in &keys {
            self.0.remove(key);
        }
        keys
    }
}

/// Holds a passkey session key alive while its connection's context lives.
pub struct ConnectionLease {
    ctx: WeakRpcContext,
    key: InternedString,
}

impl Drop for ConnectionLease {
    fn drop(&mut self) {
        let Some(ctx) = self.ctx.upgrade() else {
            return;
        };
        ctx.passkey_sessions
            .mutate(|sessions| sessions.disconnect(&self.key, Instant::now()));
        expire_after_grace(&ctx, self.key.clone());
    }
}

/// `None` for a peer that is not a passkey session key.
pub(crate) fn hold(ctx: &RpcContext, peer: &AnyVerifyingKey) -> Option<Arc<ConnectionLease>> {
    let key = peer.interned_der64();
    ctx.passkey_sessions
        .mutate(|sessions| sessions.connect(&key))
        .then(|| {
            Arc::new(ConnectionLease {
                ctx: ctx.weak(),
                key,
            })
        })
}

/// Unenrolls the key if it is still unconnected once [`GRACE`] has passed.
pub(crate) fn expire_after_grace(ctx: &RpcContext, key: InternedString) {
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let ctx = ctx.weak();
    runtime.spawn(async move {
        tokio::time::sleep(GRACE).await;
        let Some(ctx) = ctx.upgrade() else {
            return;
        };
        if ctx
            .passkey_sessions
            .mutate(|sessions| sessions.take_expired(&key, Instant::now()))
        {
            if let Err(e) = ctx.unenroll([key]).await {
                tracing::error!("{e}");
                tracing::debug!("{e:?}");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_expire_only_after_a_full_grace_without_connections() {
        let start = Instant::now();
        let key = InternedString::intern("session");
        let mut sessions = PasskeySessions::default();
        assert!(!sessions.connect(&key));
        sessions.enroll(key.clone(), "passkey".into(), start);
        assert!(!sessions.take_expired(&key, start + GRACE - Duration::from_secs(1)));

        assert!(sessions.connect(&key));
        assert!(sessions.connect(&key));
        sessions.disconnect(&key, start + GRACE);
        assert!(!sessions.take_expired(&key, start + GRACE * 3));

        sessions.disconnect(&key, start + GRACE * 3);
        assert!(!sessions.take_expired(&key, start + GRACE * 4 - Duration::from_secs(1)));
        assert!(sessions.connect(&key));
        sessions.disconnect(&key, start + GRACE * 4);
        assert!(!sessions.take_expired(&key, start + GRACE * 4 + Duration::from_secs(1)));
        assert!(sessions.take_expired(&key, start + GRACE * 5));
        assert!(!sessions.connect(&key));
    }

    #[test]
    fn revoking_a_passkey_forgets_only_its_session_keys() {
        let now = Instant::now();
        let mut sessions = PasskeySessions::default();
        sessions.enroll("a".into(), "laptop".into(), now);
        sessions.enroll("b".into(), "laptop".into(), now);
        sessions.enroll("c".into(), "phone".into(), now);
        let mut revoked = sessions.revoke_passkey(&"laptop".into());
        revoked.sort();
        assert_eq!(revoked, ["a", "b"].map(InternedString::intern));
        assert!(!sessions.connect(&"a".into()));
        assert!(sessions.connect(&"c".into()));
    }
}
