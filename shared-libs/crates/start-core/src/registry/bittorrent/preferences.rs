use serde::{Deserialize, Serialize};

use crate::prelude::*;

/// Downloads start enabled; seeding and incoming connections start disabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct TorrentPreferences {
    pub downloads: bool,
    pub seeding: bool,
    pub incoming: bool,
}

impl Default for TorrentPreferences {
    fn default() -> Self {
        Self {
            downloads: true,
            seeding: false,
            incoming: false,
        }
    }
}

impl TorrentPreferences {
    pub(crate) fn update(
        self,
        downloads: Option<bool>,
        seeding: Option<bool>,
        incoming: Option<bool>,
    ) -> Self {
        let downloads = downloads.unwrap_or(self.downloads);
        let seeding = downloads && seeding.unwrap_or(self.seeding);
        let incoming = seeding && incoming.unwrap_or(self.incoming);
        Self {
            downloads,
            seeding,
            incoming,
        }
    }
}

rpc_toolkit::reflect_ts!(TorrentPreferences);
rpc_toolkit::ts_export!(TorrentPreferences, namespaces = [""]);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_enable_only_downloads() {
        assert_eq!(
            serde_json::to_value(TorrentPreferences::default()).unwrap(),
            serde_json::json!({"downloads": true, "seeding": false, "incoming": false})
        );
    }

    #[test]
    fn disabling_a_parent_clears_its_children() {
        let enabled = TorrentPreferences {
            downloads: true,
            seeding: true,
            incoming: true,
        };
        assert_eq!(
            enabled.update(Some(false), None, None),
            TorrentPreferences {
                downloads: false,
                seeding: false,
                incoming: false,
            }
        );
        assert_eq!(
            enabled.update(None, Some(false), None),
            TorrentPreferences::default()
        );
        assert_eq!(
            enabled.update(None, None, Some(false)),
            TorrentPreferences {
                downloads: true,
                seeding: true,
                incoming: false,
            }
        );
    }

    #[test]
    fn enabling_a_parent_does_not_restore_its_children() {
        let disabled = TorrentPreferences::default().update(Some(false), None, None);
        assert_eq!(
            disabled.update(Some(true), None, None),
            TorrentPreferences::default()
        );
        assert_eq!(disabled.update(None, Some(true), Some(true)), disabled);
        assert_eq!(
            TorrentPreferences::default().update(None, None, Some(true)),
            TorrentPreferences::default()
        );
    }
}
