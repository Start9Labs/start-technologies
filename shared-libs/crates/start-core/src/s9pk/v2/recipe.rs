use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Deserialize, Serialize, visit_rs::VisitFields)]
pub struct DirRecipe(BTreeMap<PathBuf, Recipe>);

rpc_toolkit::reflect_ts!(DirRecipe);

#[derive(Debug, Clone, Deserialize, Serialize, visit_rs::VisitVariants)]
#[serde(rename_all = "camelCase")]
pub enum Recipe {
    Make(PathBuf),
    Wget { url: Url, checksum: String },
    Recipe(DirRecipe),
}

rpc_toolkit::reflect_ts!(Recipe);
