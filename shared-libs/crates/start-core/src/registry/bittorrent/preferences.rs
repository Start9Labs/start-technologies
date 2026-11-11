use serde::{Deserialize, Serialize};

use crate::prelude::*;

/// Peer downloads and stored-software sharing require explicit opt-in.
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, HasModel, VisitFields,
)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct TorrentPreferences {
    pub downloads: bool,
    pub seeding: bool,
}

impl TorrentPreferences {
    pub(crate) fn update(self, downloads: Option<bool>, seeding: Option<bool>) -> Self {
        let downloads = downloads.unwrap_or(self.downloads);
        let seeding = downloads && seeding.unwrap_or(self.seeding);
        Self { downloads, seeding }
    }
}

rpc_toolkit::reflect_ts!(TorrentPreferences);
rpc_toolkit::ts_export!(TorrentPreferences, namespaces = [""]);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_disable_peer_transfers() {
        assert_eq!(
            serde_json::to_value(TorrentPreferences::default()).unwrap(),
            serde_json::json!({"downloads": false, "seeding": false})
        );
    }

    #[test]
    fn disabling_downloads_clears_sharing() {
        let enabled = TorrentPreferences {
            downloads: true,
            seeding: true,
        };
        assert_eq!(
            enabled.update(Some(false), None),
            TorrentPreferences {
                downloads: false,
                seeding: false
            }
        );
        assert_eq!(
            enabled.update(None, Some(false)),
            TorrentPreferences {
                downloads: true,
                seeding: false
            }
        );
    }

    #[test]
    fn enabling_downloads_does_not_restore_sharing() {
        let disabled = TorrentPreferences::default().update(Some(false), None);
        assert_eq!(
            disabled.update(Some(true), None),
            TorrentPreferences {
                downloads: true,
                seeding: false
            }
        );
        assert_eq!(disabled.update(None, Some(true)), disabled);
    }
}
