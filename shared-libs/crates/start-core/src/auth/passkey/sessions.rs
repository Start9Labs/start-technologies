use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::auth::AuthKeys;
use crate::context::RpcContext;
use crate::context::rpc::WeakRpcContext;
use crate::db::model::Database;
use crate::middleware::auth::signature::{HasUnenrolledKeys, SignatureAuthorizationContext};
use crate::prelude::*;
use crate::rpc_continuations::OpenAuthedContinuations;
use crate::sign::AnyVerifyingKey;
use crate::util::sync::SyncMutex;

/// How long a passkey session key outlives its last connection.
pub const GRACE: Duration = Duration::from_secs(600);

struct Tracked {
    passkey: InternedString,
    idle_since: Option<Instant>,
}

/// Passkey session grants and physical connections, including peers not yet enrolled.
#[derive(Default)]
pub struct PasskeySessions {
    enrolled: BTreeMap<InternedString, Tracked>,
    connections: BTreeMap<InternedString, usize>,
}

impl PasskeySessions {
    pub fn enroll(&mut self, key: InternedString, passkey: InternedString, now: Instant) {
        let idle_since = (!self.connections.contains_key(&key)).then_some(now);
        self.enrolled.insert(
            key,
            Tracked {
                passkey,
                idle_since,
            },
        );
    }

    /// Records a physical connection and reports whether its key is a passkey session.
    pub fn connect(&mut self, key: &InternedString) -> bool {
        *self.connections.entry(key.clone()).or_default() += 1;
        let Some(tracked) = self.enrolled.get_mut(key) else {
            return false;
        };
        tracked.idle_since = None;
        true
    }

    /// Whether a passkey session's last physical connection closed.
    pub fn disconnect(&mut self, key: &InternedString, now: Instant) -> bool {
        let Some(connections) = self.connections.get_mut(key) else {
            return false;
        };
        *connections -= 1;
        if *connections != 0 {
            return false;
        }
        self.connections.remove(key);
        let Some(tracked) = self.enrolled.get_mut(key) else {
            return false;
        };
        tracked.idle_since = Some(now);
        true
    }

    /// Forgets and reports a key that has had no connection for [`GRACE`].
    pub fn take_expired(&mut self, key: &InternedString, now: Instant) -> bool {
        let expired = self.enrolled.get(key).is_some_and(|tracked| {
            tracked
                .idle_since
                .is_some_and(|since| now.saturating_duration_since(since) >= GRACE)
        });
        if expired {
            self.enrolled.remove(key);
        }
        expired
    }

    /// Forgets and returns the session keys the passkey enrolled.
    pub fn revoke_passkey(&mut self, passkey: &InternedString) -> Vec<InternedString> {
        let keys: Vec<_> = self
            .enrolled
            .iter()
            .filter(|(_, tracked)| tracked.passkey == *passkey)
            .map(|(key, _)| key.clone())
            .collect();
        for key in &keys {
            self.enrolled.remove(key);
        }
        keys
    }
}

/// Holds a passkey session key alive for one physical encrypted connection.
pub struct ConnectionLease {
    ctx: WeakRpcContext,
    key: InternedString,
}

impl Drop for ConnectionLease {
    fn drop(&mut self) {
        let Some(ctx) = self.ctx.upgrade() else {
            return;
        };
        if ctx
            .passkey_sessions
            .mutate(|sessions| sessions.disconnect(&self.key, Instant::now()))
        {
            expire_after_grace(&ctx, self.key.clone());
        }
    }
}

/// Tracks a physical connection even before its peer's key is enrolled.
pub(crate) fn hold(ctx: &RpcContext, peer: &AnyVerifyingKey) -> Arc<ConnectionLease> {
    let key = peer.interned_der64();
    ctx.passkey_sessions
        .mutate(|sessions| sessions.connect(&key));
    Arc::new(ConnectionLease {
        ctx: ctx.weak(),
        key,
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
        let continuations = ctx.open_authed_continuations();
        let ephemeral = &ctx.ephemeral_auth_keys;
        let sessions = &ctx.passkey_sessions;
        let result = ctx
            .db
            .mutate(|db| {
                expire_session(continuations, ephemeral, sessions, db, key, Instant::now())
            })
            .await
            .result;
        if let Err(e) = result {
            tracing::error!("{e}");
            tracing::debug!("{e:?}");
        }
    });
}

/// The caller holds the database mutation gate shared with passkey removal.
fn expire_session(
    continuations: &OpenAuthedContinuations<Option<InternedString>>,
    ephemeral: &SyncMutex<AuthKeys>,
    sessions: &SyncMutex<PasskeySessions>,
    db: &mut Model<Database>,
    key: InternedString,
    now: Instant,
) -> Result<(), Error> {
    if sessions.mutate(|sessions| sessions.take_expired(&key, now)) {
        HasUnenrolledKeys::unenroll::<RpcContext>(continuations, Some(ephemeral), db, [key])?;
    }
    Ok(())
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
        assert!(!sessions.disconnect(&key, start));
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
    fn connections_admitted_before_enrollment_keep_the_session_alive() {
        let start = Instant::now();
        let key = InternedString::intern("session");
        let mut sessions = PasskeySessions::default();
        assert!(!sessions.connect(&key));
        sessions.enroll(key.clone(), "passkey".into(), start);
        assert!(!sessions.take_expired(&key, start + GRACE * 2));
        assert!(sessions.disconnect(&key, start + GRACE * 2));
        assert!(!sessions.take_expired(&key, start + GRACE * 3 - Duration::from_secs(1)));
        assert!(sessions.take_expired(&key, start + GRACE * 3));
        assert!(sessions.connections.is_empty());
    }

    #[tokio::test]
    async fn grace_expiry_removes_tracking_and_authorization_in_the_same_mutation() {
        let db = super::super::tests::database().await;
        let continuations = OpenAuthedContinuations::new();
        let ephemeral = SyncMutex::new(AuthKeys::new());
        let sessions = SyncMutex::new(PasskeySessions::default());
        let start = Instant::now();
        let key = InternedString::intern("session");
        ephemeral.mutate(|keys| keys.0.insert(key.clone(), crate::auth::Session::default()));
        sessions.mutate(|sessions| sessions.enroll(key.clone(), "passkey".into(), start));
        for (now, enrolled) in [
            (start + GRACE - Duration::from_secs(1), true),
            (start + GRACE, false),
        ] {
            db.mutate(|db| {
                expire_session(&continuations, &ephemeral, &sessions, db, key.clone(), now)
            })
            .await
            .result
            .unwrap();
            assert_eq!(ephemeral.peek(|keys| keys.0.contains_key(&key)), enrolled);
            assert_eq!(
                sessions.peek(|sessions| sessions.enrolled.contains_key(&key)),
                enrolled
            );
        }
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
