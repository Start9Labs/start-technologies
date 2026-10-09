use std::collections::BTreeSet;

use imbl_value::InternedString;
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::prelude::*;
use crate::util::Invoke;

const KNOWN_CLASSES: &[&str] = &["processor", "display"];

#[derive(Clone, Debug, Deserialize, Serialize, VisitVariants)]
#[serde(tag = "class")]
#[serde(rename_all = "camelCase")]
pub enum LshwDevice {
    Processor(LshwProcessor),
    Display(LshwDisplay),
}

rpc_toolkit::reflect_ts!(LshwDevice);
rpc_toolkit::ts_export!(LshwDevice, namespaces = [""]);
impl LshwDevice {
    pub fn class(&self) -> &'static str {
        match self {
            Self::Processor(_) => "processor",
            Self::Display(_) => "display",
        }
    }
    #[instrument(skip_all)]
    pub fn from_value(value: &Value) -> Option<Self> {
        match value["class"].as_str() {
            Some("processor") => Some(LshwDevice::Processor(LshwProcessor::from_value(value))),
            Some("display") => Some(LshwDevice::Display(LshwDisplay::from_value(value))),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, VisitFields)]
pub struct LshwProcessor {
    pub product: Option<InternedString>,
    pub vendor: Option<InternedString>,
    pub capabilities: BTreeSet<InternedString>,
}

rpc_toolkit::reflect_ts!(LshwProcessor);
impl LshwProcessor {
    #[instrument(skip_all)]
    fn from_value(value: &Value) -> Self {
        Self {
            product: value["product"].as_str().map(From::from),
            vendor: value["vendor"].as_str().map(From::from),
            capabilities: value["capabilities"]
                .as_object()
                .into_iter()
                .flat_map(|o| o.keys())
                .map(|k| k.clone())
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, VisitFields)]
pub struct LshwDisplay {
    pub product: Option<InternedString>,
    pub vendor: Option<InternedString>,
    pub capabilities: BTreeSet<InternedString>,
    pub driver: Option<InternedString>,
}

rpc_toolkit::reflect_ts!(LshwDisplay);
impl LshwDisplay {
    #[instrument(skip_all)]
    fn from_value(value: &Value) -> Self {
        Self {
            product: value["product"].as_str().map(From::from),
            vendor: value["vendor"].as_str().map(From::from),
            capabilities: value["capabilities"]
                .as_object()
                .into_iter()
                .flat_map(|o| o.keys())
                .map(|k| k.clone())
                .collect(),
            driver: value["configuration"]["driver"].as_str().map(From::from),
        }
    }
}

pub async fn lshw() -> Result<Vec<LshwDevice>, Error> {
    let mut cmd = Command::new("lshw");
    cmd.arg("-json");
    for class in KNOWN_CLASSES {
        cmd.arg("-class").arg(*class);
    }
    Ok(
        serde_json::from_slice::<Vec<Value>>(&cmd.invoke(crate::ErrorKind::Lshw).await?)
            .with_kind(crate::ErrorKind::Deserialization)?
            .iter()
            .filter_map(LshwDevice::from_value)
            .collect(),
    )
}
