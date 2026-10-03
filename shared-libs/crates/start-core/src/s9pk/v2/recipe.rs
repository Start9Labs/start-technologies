use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use url::Url;
use visit_rs::ts::TS;

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct DirRecipe(BTreeMap<PathBuf, Recipe>);

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum Recipe {
    Make(PathBuf),
    Wget { url: Url, checksum: String },
    Recipe(DirRecipe),
}
