use std::collections::BTreeMap;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use crate::net::request_origin::RequestOrigin;
use crate::prelude::*;
use crate::util::sync::SyncMutex;
use crate::{Error, ErrorKind, GatewayId};

const WINDOW: Duration = Duration::from_secs(20);
const MAX_ATTEMPTS: usize = 3;
static LIMITER: LazyLock<SyncMutex<LoginRateLimiter>> =
    LazyLock::new(|| SyncMutex::new(LoginRateLimiter::default()));

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct LoginScope {
    address: InternedString,
    gateway: Option<GatewayId>,
    public: bool,
}

struct Attempts {
    count: usize,
    last_attempt: Instant,
    logged: bool,
}

#[derive(Default)]
struct LoginRateLimiter(BTreeMap<LoginScope, Attempts>);

impl LoginRateLimiter {
    fn check(
        &mut self,
        address: InternedString,
        origin: &RequestOrigin,
        now: Instant,
    ) -> Result<(), Error> {
        self.0
            .retain(|_, attempts| now.duration_since(attempts.last_attempt) < WINDOW);
        let scope = LoginScope {
            address,
            gateway: origin.gateway.clone(),
            public: origin.public,
        };
        let attempts = self.0.entry(scope.clone()).or_insert(Attempts {
            count: 0,
            last_attempt: now,
            logged: false,
        });
        if attempts.count >= MAX_ATTEMPTS {
            if !attempts.logged {
                attempts.logged = true;
                tracing::warn!(
                    ip = ?origin.peer_ip,
                    ingress_gateway = ?origin.gateway,
                    address = %scope.address,
                    public = origin.public,
                    "login rate limit reached"
                );
            }
            return Err(Error::new(
                eyre!("{}", t!("middleware.auth.rate-limited-login")),
                ErrorKind::RateLimited,
            ));
        }
        attempts.count += 1;
        attempts.last_attempt = now;
        Ok(())
    }
}

pub(super) fn check(address: InternedString, origin: &RequestOrigin) -> Result<(), Error> {
    LIMITER.mutate(|limiter| limiter.check(address, origin, Instant::now()))
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    use super::*;

    static TRACING_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[derive(Clone, Default)]
    struct LogOutput(Arc<Mutex<Vec<u8>>>);

    impl Write for LogOutput {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn origin(gateway: &str, public: bool) -> RequestOrigin {
        RequestOrigin {
            peer_ip: Some("203.0.113.7".parse().unwrap()),
            local_ip: Some("192.168.1.2".parse().unwrap()),
            gateway: Some(gateway.parse().unwrap()),
            public,
        }
    }

    #[test]
    fn login_rate_limits_are_scoped_by_address_gateway_and_access_not_ip() {
        let _tracing = TRACING_TEST_LOCK.lock().unwrap();
        let mut limiter = LoginRateLimiter::default();
        let now = Instant::now();
        let public = origin("eth0", true);
        for _ in 0..3 {
            limiter
                .check("server.example".into(), &public, now)
                .unwrap();
        }
        let mut other_ip = public.clone();
        other_ip.peer_ip = Some("203.0.113.8".parse().unwrap());
        assert_eq!(
            limiter
                .check("server.example".into(), &other_ip, now)
                .unwrap_err()
                .kind,
            ErrorKind::RateLimited
        );
        for (address, origin) in [
            ("other.example", public),
            ("server.example", origin("eth1", true)),
            ("server.example", origin("eth0", false)),
        ] {
            for _ in 0..3 {
                limiter.check(address.into(), &origin, now).unwrap();
            }
            assert!(limiter.check(address.into(), &origin, now).is_err());
        }
    }

    #[test]
    fn login_rate_limit_logs_once_per_reset_and_rejects_without_extending_window() {
        let _tracing = TRACING_TEST_LOCK.lock().unwrap();
        let output = LogOutput::default();
        let writer = output.clone();
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        let _subscriber = tracing::subscriber::set_default(subscriber);
        let mut limiter = LoginRateLimiter::default();
        let now = Instant::now();
        let origin = origin("eth0", true);
        for offset in [0, 1, 2] {
            limiter
                .check(
                    "server.example".into(),
                    &origin,
                    now + Duration::from_secs(offset),
                )
                .unwrap();
        }
        for offset in [3, 20, 21] {
            assert!(
                limiter
                    .check(
                        "server.example".into(),
                        &origin,
                        now + Duration::from_secs(offset)
                    )
                    .is_err()
            );
        }
        for _ in 0..3 {
            limiter
                .check(
                    "server.example".into(),
                    &origin,
                    now + Duration::from_secs(22),
                )
                .unwrap();
        }
        assert!(
            limiter
                .check(
                    "server.example".into(),
                    &origin,
                    now + Duration::from_secs(22)
                )
                .is_err()
        );
        let logs = String::from_utf8(output.0.lock().unwrap().clone()).unwrap();
        assert_eq!(logs.matches("login rate limit reached").count(), 2);
        assert!(logs.contains("203.0.113.7"));
        assert!(logs.contains("eth0"));
        assert!(logs.contains("server.example"));
        assert!(logs.contains("public=true"));
    }

    #[test]
    fn login_rate_limit_discards_expired_scopes() {
        let mut limiter = LoginRateLimiter::default();
        let now = Instant::now();
        let origin = origin("eth0", true);
        limiter.check("old.example".into(), &origin, now).unwrap();
        limiter
            .check("new.example".into(), &origin, now + WINDOW)
            .unwrap();
        assert_eq!(limiter.0.len(), 1);
        assert_eq!(limiter.0.keys().next().unwrap().address, "new.example");
    }
}
