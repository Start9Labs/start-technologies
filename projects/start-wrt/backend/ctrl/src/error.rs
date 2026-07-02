use std::fmt::{Debug, Display};

use color_eyre::eyre::eyre;
use imbl_value::Value;
use num_enum::TryFromPrimitive;
use rpc_toolkit::yajrc::{
    INVALID_PARAMS_ERROR, INVALID_REQUEST_ERROR, METHOD_NOT_FOUND_ERROR, PARSE_ERROR, RpcError,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive)]
#[repr(i32)]
pub enum ErrorKind {
    // Generic (matching startos codes)
    Unknown = 1,
    Filesystem = 2,
    NotFound = 6,
    IncorrectPassword = 7,
    Network = 9,
    Serialization = 11,
    Deserialization = 12,
    Utf8 = 13,
    Authorization = 34,
    RateLimited = 37,
    InvalidRequest = 38,
    Uninitialized = 40,
    ParseNetAddress = 41,
    ParseSshKey = 42,
    OpenSsl = 49,
    PasswordHashGeneration = 50,
    Duplicate = 53,
    MultipleErrors = 54,
    Incoherent = 55,
    Timeout = 71,
    Cancelled = 73,

    // Start-wrt domain-specific
    InterfaceNameConflict = 1000,
    LanOwnerExists = 1001,
    MissingProfile = 1002,
    CorruptedProfile = 1003,
    CorruptedWifi = 1004,
    UnnamedWirelessInterface = 1005,
    UnnamedWirelessDevice = 1006,
    DuplicateVlanTag = 1007,
    CannotDeleteLanOwner = 1008,
    MissingLanBridge = 1009,
    MissingWanInterface = 1010,
    MissingLanInterface = 1011,
    MissingFirewallZone = 1012,
    WanPortWithProfile = 1013,
    DuplicatePassword = 1014,
    DuplicatePasswordLabel = 1015,
    VpnHasDependents = 1016,
    VpnChainCycle = 1017,
    DuplicateFullname = 1018,
    VpnPeersWouldBreak = 1019,
    MissingDeviceAddress = 1020,
    InvalidValue = 1021,
    UciEdit = 1022,
    DhcpStaticHostsInSubnet = 1023,
    SubnetCollision = 1024,
}

impl From<ErrorKind> for startos::ErrorKind {
    fn from(k: ErrorKind) -> Self {
        // Matching numeric code where possible (generic variants have the same
        // codes as startos); domain-specific variants (>=1000) fall back to Unknown.
        startos::ErrorKind::try_from(k as i32).unwrap_or(startos::ErrorKind::Unknown)
    }
}

impl ErrorKind {
    pub fn as_str(&self) -> &'static str {
        use ErrorKind::*;
        match self {
            Unknown => "Unknown Error",
            Filesystem => "Filesystem Error",
            NotFound => "Not Found",
            IncorrectPassword => "Incorrect Password",
            Network => "Network Error",
            Serialization => "Serialization Error",
            Deserialization => "Deserialization Error",
            Utf8 => "UTF-8 Error",
            Authorization => "Authorization Error",
            RateLimited => "Rate Limited",
            InvalidRequest => "Invalid Request",
            Uninitialized => "Uninitialized",
            ParseNetAddress => "Invalid Network Address",
            ParseSshKey => "Invalid SSH Key",
            OpenSsl => "OpenSSL Error",
            PasswordHashGeneration => "Password Hash Generation Error",
            Duplicate => "Duplicate",
            MultipleErrors => "Multiple Errors",
            Incoherent => "Incoherent State",
            Timeout => "Timeout",
            Cancelled => "Cancelled",
            InterfaceNameConflict => "Interface Name Conflict",
            LanOwnerExists => "LAN Owner Already Exists",
            MissingProfile => "Missing Profile",
            CorruptedProfile => "Corrupted Profile",
            CorruptedWifi => "Corrupted WiFi",
            UnnamedWirelessInterface => "Unnamed Wireless Interface",
            UnnamedWirelessDevice => "Unnamed Wireless Device",
            DuplicateVlanTag => "Duplicate VLAN Tag",
            CannotDeleteLanOwner => "Cannot Delete LAN Owner",
            MissingLanBridge => "Missing LAN Bridge",
            MissingWanInterface => "Missing WAN Interface",
            MissingLanInterface => "Missing LAN Interface",
            MissingFirewallZone => "Missing Firewall Zone",
            WanPortWithProfile => "WAN Port Cannot Have Profile",
            DuplicatePassword => "Duplicate WiFi Password",
            DuplicatePasswordLabel => "Duplicate WiFi Password Label",
            VpnHasDependents => "VPN Has Dependents",
            VpnChainCycle => "VPN Chain Cycle",
            DuplicateFullname => "Duplicate Profile Name",
            VpnPeersWouldBreak => "VPN Peers Would Break",
            MissingDeviceAddress => "Missing Device Address",
            InvalidValue => "Invalid Value",
            UciEdit => "UCI Edit Error",
            DhcpStaticHostsInSubnet => "DHCP Static Hosts in Subnet",
            SubnetCollision => "Subnet Collision",
        }
    }
}

impl Display for ErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

pub struct Error {
    pub source: color_eyre::eyre::Error,
    pub kind: ErrorKind,
    pub info: Value,
}


impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {:#}", self.kind.as_str(), self.source)
    }
}

impl Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {:?}", self.kind.as_str(), self.source)
    }
}

impl std::error::Error for Error {}

impl Error {
    pub fn new<E: Into<color_eyre::eyre::Error> + std::fmt::Debug + 'static>(
        source: E,
        kind: ErrorKind,
    ) -> Self {
        Error {
            source: source.into(),
            kind,
            info: Value::Null,
        }
    }

    pub fn with_info(mut self, info: Value) -> Self {
        self.info = info;
        self
    }

    /// Shorthand for ad-hoc errors where no specific `ErrorKind` applies.
    /// Wraps the message via `eyre!` and tags as `ErrorKind::Unknown`. Used
    /// extensively by the OTA update path (registry RPC, signature
    /// verification, sysupgrade invocation).
    pub fn other(msg: impl Display) -> Self {
        Error::new(eyre!("{msg}"), ErrorKind::Unknown)
    }
}

// --- From impls ---

impl From<std::convert::Infallible> for Error {
    fn from(value: std::convert::Infallible) -> Self {
        match value {}
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::new(e, ErrorKind::Filesystem)
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(e: std::str::Utf8Error) -> Self {
        Error::new(e, ErrorKind::Utf8)
    }
}

impl From<std::string::FromUtf8Error> for Error {
    fn from(e: std::string::FromUtf8Error) -> Self {
        Error::new(e, ErrorKind::Utf8)
    }
}

impl From<std::net::AddrParseError> for Error {
    fn from(e: std::net::AddrParseError) -> Self {
        Error::new(e, ErrorKind::ParseNetAddress)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::new(e, ErrorKind::Deserialization)
    }
}

impl From<uciedit::Error> for Error {
    fn from(e: uciedit::Error) -> Self {
        Error::new(eyre!("{e}"), ErrorKind::UciEdit)
    }
}

impl From<openssl::error::ErrorStack> for Error {
    fn from(e: openssl::error::ErrorStack) -> Self {
        Error::new(eyre!("{e}"), ErrorKind::OpenSsl)
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        let kind = if e.is_builder() {
            ErrorKind::InvalidRequest
        } else if e.is_decode() {
            ErrorKind::Deserialization
        } else {
            ErrorKind::Network
        };
        Error::new(e, kind)
    }
}


impl From<tokio::task::JoinError> for Error {
    fn from(e: tokio::task::JoinError) -> Self {
        Error::new(eyre!("{e}"), ErrorKind::Unknown)
    }
}

/// Convert a startos::Error to our Error. Uses the numeric ErrorKind code
/// (since both enums are #[repr(i32)] and the generic variants match by value).
impl From<startos::Error> for Error {
    fn from(e: startos::Error) -> Self {
        let code = e.kind as i32;
        let kind = ErrorKind::try_from(code).unwrap_or(ErrorKind::Unknown);
        Error {
            source: eyre!("{}", e),
            kind,
            info: e.info,
        }
    }
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::new(eyre!("{e}"), ErrorKind::Filesystem)
    }
}

// --- RPC conversion ---

#[derive(Clone, Deserialize, Serialize)]
pub struct ErrorData {
    pub details: String,
    #[serde(default)]
    pub info: Value,
}

impl Display for ErrorData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.details, f)
    }
}
impl Debug for ErrorData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.details, f)
    }
}
impl std::error::Error for ErrorData {}

impl From<Error> for ErrorData {
    fn from(value: Error) -> Self {
        Self {
            details: format!("{:#}", value.source),
            info: value.info,
        }
    }
}

impl From<&RpcError> for ErrorData {
    fn from(value: &RpcError) -> Self {
        Self {
            details: value
                .data
                .as_ref()
                .and_then(|d| {
                    d.as_object()
                        .and_then(|d| {
                            d.get("details")
                                .and_then(|d| d.as_str().map(|s| s.to_owned()))
                        })
                        .or_else(|| d.as_str().map(|s| s.to_owned()))
                })
                .unwrap_or_else(|| value.message.clone().into_owned()),
            info: imbl_value::to_value(
                &value
                    .data
                    .as_ref()
                    .and_then(|d| d.as_object().and_then(|d| d.get("info"))),
            )
            .unwrap_or_default(),
        }
    }
}

impl From<RpcError> for Error {
    fn from(e: RpcError) -> Self {
        let data = ErrorData::from(&e);
        let info = data.info.clone();
        Error::new(
            data,
            if let Ok(kind) = e.code.try_into() {
                kind
            } else if e.code == METHOD_NOT_FOUND_ERROR.code {
                ErrorKind::NotFound
            } else if e.code == PARSE_ERROR.code
                || e.code == INVALID_PARAMS_ERROR.code
                || e.code == INVALID_REQUEST_ERROR.code
            {
                ErrorKind::Deserialization
            } else {
                ErrorKind::Unknown
            },
        )
        .with_info(info)
    }
}

// --- Extension traits ---

pub trait ResultExt<T, E>
where
    Self: Sized,
{
    fn with_kind(self, kind: ErrorKind) -> Result<T, Error>;
    fn with_ctx<F: FnOnce(&E) -> (ErrorKind, D), D: Display>(self, f: F) -> Result<T, Error>;
    fn log_err(self) -> Option<T>;
}

impl<T, E> ResultExt<T, E> for Result<T, E>
where
    color_eyre::eyre::Error: From<E>,
    E: std::fmt::Debug + 'static,
{
    fn with_kind(self, kind: ErrorKind) -> Result<T, Error> {
        self.map_err(|e| Error::new(e, kind))
    }

    fn with_ctx<F: FnOnce(&E) -> (ErrorKind, D), D: Display>(self, f: F) -> Result<T, Error> {
        self.map_err(|e| {
            let (kind, ctx) = f(&e);
            let source = color_eyre::eyre::Error::from(e);
            let with_ctx = format!("{ctx}: {source}");
            let source = source.wrap_err(with_ctx);
            Error {
                kind,
                source,
                info: Value::Null,
            }
        })
    }

    fn log_err(self) -> Option<T> {
        match self {
            Ok(a) => Some(a),
            Err(e) => {
                let e: color_eyre::eyre::Error = e.into();
                tracing::error!("{e}");
                tracing::debug!("{e:?}");
                None
            }
        }
    }
}


pub trait OptionExt<T>
where
    Self: Sized,
{
    fn or_not_found(self, message: impl std::fmt::Display) -> Result<T, Error>;
}

impl<T> OptionExt<T> for Option<T> {
    fn or_not_found(self, message: impl std::fmt::Display) -> Result<T, Error> {
        self.ok_or_else(|| Error::new(eyre!("{}", message), ErrorKind::NotFound))
    }
}

#[derive(Debug, Default)]
pub struct ErrorCollection(Vec<Error>);

impl ErrorCollection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle<T, E: Into<Error>>(&mut self, result: Result<T, E>) -> Option<T> {
        match result {
            Ok(a) => Some(a),
            Err(e) => {
                self.0.push(e.into());
                None
            }
        }
    }

    pub fn into_result(self) -> Result<(), Error> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(Error::new(eyre!("{}", self), ErrorKind::MultipleErrors))
        }
    }
}

impl From<ErrorCollection> for Result<(), Error> {
    fn from(e: ErrorCollection) -> Self {
        e.into_result()
    }
}

impl<T, E: Into<Error>> Extend<Result<T, E>> for ErrorCollection {
    fn extend<I: IntoIterator<Item = Result<T, E>>>(&mut self, iter: I) {
        for item in iter {
            self.handle(item);
        }
    }
}

impl Display for ErrorCollection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (idx, e) in self.0.iter().enumerate() {
            if idx > 0 {
                write!(f, "; ")?;
            }
            write!(f, "{}", e)?;
        }
        Ok(())
    }
}

#[macro_export]
macro_rules! ensure_code {
    ($x:expr, $c:expr, $fmt:expr $(, $arg:expr)*) => {
        if !($x) {
            return Err($crate::error::Error::new(
                $crate::prelude::eyre!($fmt, $($arg, )*),
                $c,
            ));
        }
    };
}
