use std::str::FromStr;

use clap::builder::ValueParserFactory;
use serde::{Deserialize, Serialize};
use visit_rs::ts::TS;

pub use crate::HealthCheckId;
use crate::util::FromStrParser;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
pub struct NamedHealthCheckResult {
    pub name: String,
    #[serde(flatten)]
    pub kind: NamedHealthCheckResultKind,
}
// healthCheckName:kind:message OR healthCheckName:kind
impl FromStr for NamedHealthCheckResult {
    type Err = color_eyre::eyre::Report;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let from_parts = |name: &str, kind: &str, message: Option<&str>| {
            let message = message.map(|x| x.to_string());
            let kind = match kind {
                "success" => NamedHealthCheckResultKind::Success { message },
                "disabled" => NamedHealthCheckResultKind::Disabled { message },
                "starting" => NamedHealthCheckResultKind::Starting { message },
                "waiting" => NamedHealthCheckResultKind::Waiting { message },
                "loading" => NamedHealthCheckResultKind::Loading {
                    message: message.unwrap_or_default(),
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
        let parts = s.split(':').collect::<Vec<_>>();
        match &*parts {
            [name, kind, message] => from_parts(name, kind, Some(message)),
            [name, kind] => from_parts(name, kind, None),
            _ => Err(color_eyre::eyre::eyre!(
                "Could not match the shape of the result ${parts:?}"
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "result")]
pub enum NamedHealthCheckResultKind {
    Success { message: Option<String> },
    Disabled { message: Option<String> },
    Starting { message: Option<String> },
    Waiting { message: Option<String> },
    Loading { message: String },
    Failure { message: String },
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
            NamedHealthCheckResultKind::Starting { message } => {
                if let Some(message) = message {
                    write!(f, "{name}: Starting ({message})")
                } else {
                    write!(f, "{name}: Starting")
                }
            }
            NamedHealthCheckResultKind::Waiting { message } => {
                if let Some(message) = message {
                    write!(f, "{name}: Waiting ({message})")
                } else {
                    write!(f, "{name}: Waiting")
                }
            }
            NamedHealthCheckResultKind::Loading { message } => {
                write!(f, "{name}: Loading ({message})")
            }
            NamedHealthCheckResultKind::Failure { message } => {
                write!(f, "{name}: Failed ({message})")
            }
        }
    }
}
