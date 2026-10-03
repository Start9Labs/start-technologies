use std::collections::BTreeSet;
use std::str::FromStr;
use std::sync::{Arc, Weak};
use std::time::Duration;

use clap::builder::ValueParserFactory;
use exver::{ExtendedVersion, VersionRange};
use imbl::Vector;
use imbl_value::{InternedString, Value};
use rpc_toolkit::Empty;
use rpc_toolkit::yajrc::RpcMethod;
use visit_rs::ts::TS;

use crate::prelude::*;
use crate::rpc_continuations::Guid;
use crate::service::ProcedureName;
use crate::service::persistent_container::PersistentContainer;
use crate::util::{FromStrParser, Never};

#[derive(Clone, serde::Deserialize, serde::Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum InitKind {
    Install,
    Update,
    Restore,
}

#[derive(Clone, serde::Deserialize, serde::Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct InitParams {
    pub id: Guid,
    pub kind: Option<InitKind>,
}

#[derive(Clone)]
pub struct Init;
impl RpcMethod for Init {
    type Params = InitParams;
    type Response = ();
    fn as_str<'a>(&'a self) -> &'a str {
        "init"
    }
}
impl serde::Serialize for Init {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[derive(Clone)]
pub struct Start;
impl RpcMethod for Start {
    type Params = Empty;
    type Response = ();
    fn as_str<'a>(&'a self) -> &'a str {
        "start"
    }
}
impl serde::Serialize for Start {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[derive(Clone)]
pub struct Stop;
impl RpcMethod for Stop {
    type Params = Empty;
    type Response = ();
    fn as_str<'a>(&'a self) -> &'a str {
        "stop"
    }
}
impl serde::Serialize for Stop {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[derive(Clone, serde::Deserialize, serde::Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExitParams {
    id: Guid,
    /// VersionRange or ExtendedVersion
    #[ts(type = "string | null")]
    target: Option<InternedString>,
}
impl ExitParams {
    pub fn target_version(version: &ExtendedVersion) -> Self {
        Self {
            id: Guid::new(),
            target: Some(InternedString::from_display(version)),
        }
    }
    pub fn target_range(range: &VersionRange) -> Self {
        Self {
            id: Guid::new(),
            target: Some(InternedString::from_display(range)),
        }
    }
    pub fn target_str(s: &str) -> Self {
        Self {
            id: Guid::new(),
            target: Some(InternedString::intern(s)),
        }
    }
    pub fn uninstall() -> Self {
        Self {
            id: Guid::new(),
            target: None,
        }
    }
    pub fn is_uninstall(&self) -> bool {
        self.target.is_none()
    }
}

#[derive(Clone)]
pub struct Exit;
impl RpcMethod for Exit {
    type Params = ExitParams;
    type Response = ();
    fn as_str<'a>(&'a self) -> &'a str {
        "exit"
    }
}
impl serde::Serialize for Exit {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[derive(Clone, serde::Deserialize, serde::Serialize, TS)]
pub struct ExecuteParams {
    id: Guid,
    procedure: String,
    input: Value,
    timeout: Option<u128>,
}
impl ExecuteParams {
    pub fn new(
        id: Guid,
        procedure: ProcedureName,
        input: Value,
        timeout: Option<Duration>,
    ) -> Self {
        Self {
            id,
            procedure: procedure.js_function_name(),
            input,
            timeout: timeout.map(|d| d.as_millis()),
        }
    }
}

#[derive(Clone)]
pub struct Execute;
impl RpcMethod for Execute {
    type Params = ExecuteParams;
    type Response = Value;
    fn as_str<'a>(&'a self) -> &'a str {
        "execute"
    }
}
impl serde::Serialize for Execute {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[derive(Clone)]
pub struct Sandbox;
impl RpcMethod for Sandbox {
    type Params = ExecuteParams;
    type Response = Value;
    fn as_str<'a>(&'a self) -> &'a str {
        "sandbox"
    }
}
impl serde::Serialize for Sandbox {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[derive(
    Clone, Copy, Debug, serde::Deserialize, serde::Serialize, TS, PartialEq, Eq, PartialOrd, Ord,
)]
#[ts(type = "number")]
pub struct CallbackId(u64);
impl CallbackId {
    pub fn register(self, container: &PersistentContainer) -> CallbackHandle {
        let this = Arc::new(self);
        let res = Arc::downgrade(&this);
        container
            .state
            .send_if_modified(|s| s.callbacks.insert(this));
        CallbackHandle(res)
    }
}
impl FromStr for CallbackId {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        u64::from_str(s).map_err(Error::from).map(Self)
    }
}
impl ValueParserFactory for CallbackId {
    type Parser = FromStrParser<Self>;
    fn value_parser() -> Self::Parser {
        FromStrParser::new()
    }
}

pub struct CallbackHandle(Weak<CallbackId>);
impl CallbackHandle {
    pub fn is_active(&self) -> bool {
        self.0.strong_count() > 0
    }
    pub fn params(
        self,
        registered: &mut BTreeSet<Arc<CallbackId>>,
        args: Vector<Value>,
    ) -> Option<CallbackParams> {
        if let Some(id) = self.0.upgrade() {
            if let Some(strong) = registered.get(&id) {
                if Arc::ptr_eq(strong, &id) {
                    registered.remove(&id);
                    return Some(CallbackParams::new(&*id, args));
                }
            }
        }
        None
    }
    pub fn take(&mut self) -> Self {
        Self(std::mem::take(&mut self.0))
    }
}

#[derive(Clone, serde::Deserialize, serde::Serialize, TS)]
pub struct CallbackParams {
    id: u64,
    #[ts(type = "any[]")]
    args: Vector<Value>,
}
impl CallbackParams {
    fn new(id: &CallbackId, args: Vector<Value>) -> Self {
        Self { id: id.0, args }
    }
}

#[derive(Clone)]
pub struct Callback;
impl RpcMethod for Callback {
    type Params = CallbackParams;
    type Response = Never;
    fn as_str<'a>(&'a self) -> &'a str {
        "callback"
    }
}
impl serde::Serialize for Callback {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}
