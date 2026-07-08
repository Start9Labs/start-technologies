/// This module was designed to address the problem of releasing updates to Embassy Packages where the upstream project was
/// either unaware of or apathetic towards supporting their application on the Embassy platform. In most cases, the original
/// package will support <https://semver.org/spec/v2.0.0.html semver2>. This leaves us with the problem where we would like
/// to preserve the original package's version, since one of the goals of the Embassy platform is transparency. However, on
/// occasion, we have screwed up and published a version of a package that needed to have its metadata updated. In this
/// scenario we were left with the conundrum of either unilaterally claiming a version number of a package we did not author
/// or let the issue persist until the next update. Neither of these promote good user experiences, for different reasons.
/// This module extends the semver standard linked above with a 4th digit, which is given PATCH semantics.
pub mod exver;
pub use emver;

pub use crate::exver::*;

#[cfg(feature = "wasm")]
pub mod wasm;

#[cfg(test)]
mod test;
