pub use clap;
pub use cli::*;
// pub use command::*;
pub use context::*;
pub use futures;
pub use handler::*;
pub use reqwest;
pub use serde;
pub use serde_json;
pub use server::*;
pub use tokio;
pub use url;
pub use yajrc;

mod cli;
pub mod command_helpers;
mod context;
mod handler;
mod server;
#[cfg(feature = "ts")]
pub mod ts;

pub mod util;

#[cfg(not(feature = "ts"))]
pub mod ts {
    pub use crate::{impl_ts, impl_ts_array, impl_ts_map, impl_ts_shape, reflect_ts, ts_export};

    pub trait HandlerTSBindings {}
    impl<T> HandlerTSBindings for T {}
    pub trait TS {}
    impl<T: ?Sized> TS for T {}

    #[derive(Debug, Clone, Copy)]
    pub struct Unknown;
    #[derive(Debug, Clone, Copy)]
    pub enum Never {}
}

#[cfg(not(feature = "ts"))]
mod ts_disabled {
    #[macro_export]
    macro_rules! reflect_ts {
        ($($tokens:tt)*) => {};
    }
    #[macro_export]
    macro_rules! ts_export {
        ($($tokens:tt)*) => {};
    }
    #[macro_export]
    macro_rules! impl_ts_shape {
        ($($tokens:tt)*) => {};
    }
    #[macro_export]
    macro_rules! impl_ts {
        ($($tokens:tt)*) => {};
    }
    #[macro_export]
    macro_rules! impl_ts_map {
        ($($tokens:tt)*) => {};
    }
    #[macro_export]
    macro_rules! impl_ts_array {
        ($($tokens:tt)*) => {};
    }
}
