pub use color_eyre::eyre::eyre;
pub use imbl_value::InternedString;
pub use lazy_format::lazy_format;
pub use rust_i18n::t;
pub use tracing::instrument;

pub use crate::db::prelude::*;
pub use crate::ensure_code;
pub use crate::error::{Error, ErrorCollection, ErrorKind, OptionExt, ResultExt};

#[macro_export]
macro_rules! dbg {
    () => {{
        tracing::debug!("[{}:{}:{}]", file!(), line!(), column!());
    }};
    ($e:expr) => {{
        let e = $e;
        tracing::debug!("[{}:{}:{}] {} = {e:?}", file!(), line!(), column!(), stringify!($e));
        e
    }};
    ($($e:expr),+) => {
        ($(
            crate::dbg!($e)
        ),+)
    }
}

/// Emit a tracing event only in `dev` builds (the `dev` cargo feature). For
/// best-effort/expected failures — e.g. PCP/UPnP/NAT-PMP port-mapping attempts
/// against gateways that don't support them — which are pure noise in
/// production but useful when developing. `$level` is a tracing level macro
/// name (`debug`/`warn`/`error`/…); `if cfg!` keeps the arguments type-checked
/// while release builds drop the branch.
#[macro_export]
macro_rules! dev_log {
    ($level:ident, $($arg:tt)*) => {
        if cfg!(feature = "dev") {
            tracing::$level!($($arg)*);
        }
    };
}
