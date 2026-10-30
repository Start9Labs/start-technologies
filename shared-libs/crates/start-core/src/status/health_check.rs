use std::str::FromStr;

use clap::builder::ValueParserFactory;
use serde::{Deserialize, Serialize};

pub use crate::HealthCheckId;
use crate::util::FromStrParser;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct NamedHealthCheckResult {
    pub name: String,
    #[serde(flatten, deserialize_with = "legacy_waiting::deserialize")]
    #[visit(input_wire = "legacy_waiting::CompatibleHealthCheckResultKind")]
    #[visit(type_attributes(visit::input_wire))]
    pub kind: NamedHealthCheckResultKind,
}

rpc_toolkit::reflect_ts!(NamedHealthCheckResult);
impl FromStr for NamedHealthCheckResult {
    type Err = color_eyre::eyre::Report;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let from_parts = |name: &str, kind: &str, message: Option<&str>| {
            let message = message.map(|x| x.to_string());
            let kind = match kind {
                "success" => NamedHealthCheckResultKind::Success { message },
                "disabled" => NamedHealthCheckResultKind::Disabled { message },
                "starting" => NamedHealthCheckResultKind::Starting {
                    message,
                    waiting: false,
                },
                "waiting" => NamedHealthCheckResultKind::waiting(message),
                "loading" => NamedHealthCheckResultKind::Loading {
                    message: message.unwrap_or_default(),
                    waiting: false,
                },
                "failure" => NamedHealthCheckResultKind::Failure {
                    message: message.unwrap_or_default(),
                },
                _ => return Err(color_eyre::eyre::eyre!("Invalid health check kind")),
            };
            Ok(Self {
                name: name.to_string(),
                kind,
            })
        };
        let parts = s.splitn(3, ':').collect::<Vec<_>>();
        match &*parts {
            [name, kind, message] => from_parts(name, kind, Some(message)),
            [name, kind] => from_parts(name, kind, None),
            _ => Err(color_eyre::eyre::eyre!(
                "Could not match the shape of the result {parts:?}"
            )),
        }
    }
}
impl ValueParserFactory for NamedHealthCheckResult {
    type Parser = FromStrParser<Self>;
    fn value_parser() -> Self::Parser {
        FromStrParser::new()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, visit_rs::VisitVariants)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "result")]
pub enum NamedHealthCheckResultKind {
    Success {
        message: Option<String>,
    },
    Disabled {
        message: Option<String>,
    },
    Starting {
        message: Option<String>,
        #[serde(default)]
        waiting: bool,
    },
    Loading {
        message: String,
        #[serde(default)]
        waiting: bool,
    },
    Failure {
        message: String,
    },
}

rpc_toolkit::reflect_ts!(NamedHealthCheckResultKind);

impl NamedHealthCheckResultKind {
    fn waiting(message: Option<String>) -> Self {
        Self::Starting {
            message,
            waiting: true,
        }
    }
}

impl std::fmt::Display for NamedHealthCheckResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = &self.name;
        match &self.kind {
            NamedHealthCheckResultKind::Success { message } => {
                if let Some(message) = message {
                    write!(f, "{name}: Succeeded ({message})")
                } else {
                    write!(f, "{name}: Succeeded")
                }
            }
            NamedHealthCheckResultKind::Disabled { message } => {
                if let Some(message) = message {
                    write!(f, "{name}: Disabled ({message})")
                } else {
                    write!(f, "{name}: Disabled")
                }
            }
            NamedHealthCheckResultKind::Starting { message, .. } => {
                if let Some(message) = message {
                    write!(f, "{name}: Starting ({message})")
                } else {
                    write!(f, "{name}: Starting")
                }
            }
            NamedHealthCheckResultKind::Loading { message, .. } => {
                write!(f, "{name}: Loading ({message})")
            }
            NamedHealthCheckResultKind::Failure { message } => {
                write!(f, "{name}: Failed ({message})")
            }
        }
    }
}

mod legacy_waiting {
    use serde::{Deserialize, Deserializer};

    use super::NamedHealthCheckResultKind;

    #[derive(Deserialize, visit_rs::VisitVariants)]
    #[serde(rename_all = "camelCase", tag = "result")]
    pub(super) enum LegacyHealthCheckResultKind {
        Waiting { message: Option<String> },
    }

    rpc_toolkit::reflect_ts!(LegacyHealthCheckResultKind);

    #[derive(Deserialize, visit_rs::VisitVariants)]
    #[serde(untagged)]
    pub(super) enum CompatibleHealthCheckResultKind {
        Current(NamedHealthCheckResultKind),
        Legacy(LegacyHealthCheckResultKind),
    }

    rpc_toolkit::reflect_ts!(CompatibleHealthCheckResultKind);

    pub(super) fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<NamedHealthCheckResultKind, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(
            match CompatibleHealthCheckResultKind::deserialize(deserializer)? {
                CompatibleHealthCheckResultKind::Current(kind) => kind,
                CompatibleHealthCheckResultKind::Legacy(LegacyHealthCheckResultKind::Waiting {
                    message,
                }) => NamedHealthCheckResultKind::waiting(message),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn waiting_is_a_flag_on_starting_and_loading() {
        for result in ["starting", "loading"] {
            for waiting in [false, true] {
                let wire = json!({
                    "name": "Sync",
                    "result": result,
                    "waiting": waiting,
                    "message": "Sync: <25%>...",
                });
                let check: NamedHealthCheckResult = serde_json::from_value(wire.clone()).unwrap();
                assert_eq!(serde_json::to_value(check).unwrap(), wire);
            }

            let check: NamedHealthCheckResult = serde_json::from_value(json!({
                "name": "Sync",
                "result": result,
                "message": "",
            }))
            .unwrap();
            assert_eq!(serde_json::to_value(check).unwrap()["waiting"], false);
        }
    }

    #[test]
    fn legacy_waiting_normalizes_to_starting() {
        for message in [None, Some("Database: unavailable")] {
            let wire = json!({ "name": "App", "result": "waiting", "message": message });
            let check: NamedHealthCheckResult = serde_json::from_value(wire).unwrap();
            let expected = json!({
                "name": "App",
                "result": "starting",
                "waiting": true,
                "message": message,
            });
            assert_eq!(serde_json::to_value(check).unwrap(), expected);

            let cli = match message {
                Some(message) => format!("App:waiting:{message}"),
                None => "App:waiting".to_owned(),
            };
            let check: NamedHealthCheckResult = cli.parse().unwrap();
            assert_eq!(serde_json::to_value(check).unwrap(), expected);
        }
    }

    #[test]
    fn every_status_preserves_arbitrary_messages() {
        for result in ["success", "disabled", "starting", "loading", "failure"] {
            for message in ["", "  Status: <ready> & waiting...\nDetails  "] {
                let check: NamedHealthCheckResult = serde_json::from_value(json!({
                    "name": "Probe",
                    "result": result,
                    "message": message,
                }))
                .unwrap();
                assert_eq!(serde_json::to_value(check).unwrap()["message"], message);

                let check: NamedHealthCheckResult =
                    format!("Probe:{result}:{message}").parse().unwrap();
                assert_eq!(serde_json::to_value(check).unwrap()["message"], message);
            }
        }
    }
}
