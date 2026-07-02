//! Re-export of `startos::util::Invoke` so local code uses the canonical
//! implementation. Call `.invoke(ErrorKind::X.into())` to pass our local
//! ErrorKind (conversion via `From<crate::ErrorKind> for startos::ErrorKind`).
//!
//! The returned `Result<Vec<u8>, startos::Error>` auto-converts to our
//! `Result<Vec<u8>, Error>` via `?` because we have `From<startos::Error>`.

pub use startos::util::{ExtendedCommand, Invoke};
