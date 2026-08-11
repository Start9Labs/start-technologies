use std::fmt::{Debug, Display};

use axum::http::StatusCode;
use axum::http::uri::InvalidUri;
use color_eyre::eyre::eyre;
use imbl_value::InternedString;
use num_enum::TryFromPrimitive;
use patch_db::Value;
use rpc_toolkit::reqwest;
use rpc_toolkit::yajrc::{
    INVALID_PARAMS_ERROR, INVALID_REQUEST_ERROR, METHOD_NOT_FOUND_ERROR, PARSE_ERROR, RpcError,
};
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;
use tokio_rustls::rustls;
use ts_rs::TS;

use crate::InvalidId;
use crate::prelude::to_value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive)]
#[repr(i32)]
pub enum ErrorKind {
    Unknown = 1,
    Filesystem = 2,
    Docker = 3,
    ConfigSpecViolation = 4,
    ConfigRulesViolation = 5,
    NotFound = 6,
    IncorrectPassword = 7,
    VersionIncompatible = 8,
    Network = 9,
    Registry = 10,
    Serialization = 11,
    Deserialization = 12,
    Utf8 = 13,
    ParseVersion = 14,
    IncorrectDisk = 15,
    // Nginx = 16,
    Dependency = 17,
    ParseS9pk = 18,
    ParseUrl = 19,
    DiskNotAvailable = 20,
    BlockDevice = 21,
    // InvalidOnionAddress = 22,
    Pack = 23,
    ValidateS9pk = 24,
    DiskCorrupted = 25, // Remove
    // Tor = 26,
    ConfigGen = 27,
    ParseNumber = 28,
    Database = 29,
    InvalidId = 30,
    InvalidSignature = 31,
    Backup = 32,
    Restore = 33,
    Authorization = 34,
    AutoConfigure = 35,
    Action = 36,
    RateLimited = 37,
    InvalidRequest = 38,
    MigrationFailed = 39,
    Uninitialized = 40,
    ParseNetAddress = 41,
    ParseSshKey = 42,
    SoundError = 43,
    ParseTimestamp = 44,
    ParseSysInfo = 45,
    Wifi = 46,
    Journald = 47,
    DiskManagement = 48,
    OpenSsl = 49,
    PasswordHashGeneration = 50,
    DiagnosticMode = 51,
    ParseDbField = 52,
    Duplicate = 53,
    MultipleErrors = 54,
    Incoherent = 55,
    InvalidBackupTargetId = 56,
    ProductKeyMismatch = 57,
    LanPortConflict = 58,
    ServiceRuntime = 59,
    Pem = 60,
    TLSInit = 61,
    Ascii = 62,
    MissingHeader = 63,
    Grub = 64,
    Systemd = 65,
    OpenSsh = 66,
    Zram = 67,
    Lshw = 68,
    CpuSettings = 69,
    Firmware = 70,
    Timeout = 71,
    Lxc = 72,
    Cancelled = 73,
    Git = 74,
    DBus = 75,
    InstallFailed = 76,
    UpdateFailed = 77,
    Smtp = 78,
    SetSysInfo = 79,
    Bios = 80,
    BackupPasswordMismatch = 81,
}
impl ErrorKind {
    pub fn as_str(&self) -> String {
        use ErrorKind::*;
        match self {
            Unknown => t!("error.unknown"),
            Filesystem => t!("error.filesystem"),
            Docker => t!("error.docker"),
            ConfigSpecViolation => t!("error.config-spec-violation"),
            ConfigRulesViolation => t!("error.config-rules-violation"),
            NotFound => t!("error.not-found"),
            IncorrectPassword => t!("error.incorrect-password"),
            VersionIncompatible => t!("error.version-incompatible"),
            Network => t!("error.network"),
            Registry => t!("error.registry"),
            Serialization => t!("error.serialization"),
            Deserialization => t!("error.deserialization"),
            Utf8 => t!("error.utf8"),
            ParseVersion => t!("error.parse-version"),
            IncorrectDisk => t!("error.incorrect-disk"),
            // Nginx => t!("error.nginx"),
            Dependency => t!("error.dependency"),
            ParseS9pk => t!("error.parse-s9pk"),
            ParseUrl => t!("error.parse-url"),
            DiskNotAvailable => t!("error.disk-not-available"),
            BlockDevice => t!("error.block-device"),
            // InvalidOnionAddress => t!("error.invalid-onion-address"),
            Pack => t!("error.pack"),
            ValidateS9pk => t!("error.validate-s9pk"),
            DiskCorrupted => t!("error.disk-corrupted"), // Remove
            // Tor => t!("error.tor"),
            ConfigGen => t!("error.config-gen"),
            ParseNumber => t!("error.parse-number"),
            Database => t!("error.database"),
            InvalidId => t!("error.invalid-id"),
            InvalidSignature => t!("error.invalid-signature"),
            Backup => t!("error.backup"),
            Restore => t!("error.restore"),
            Authorization => t!("error.authorization"),
            AutoConfigure => t!("error.auto-configure"),
            Action => t!("error.action"),
            RateLimited => t!("error.rate-limited"),
            InvalidRequest => t!("error.invalid-request"),
            MigrationFailed => t!("error.migration-failed"),
            Uninitialized => t!("error.uninitialized"),
            ParseNetAddress => t!("error.parse-net-address"),
            ParseSshKey => t!("error.parse-ssh-key"),
            SoundError => t!("error.sound-error"),
            ParseTimestamp => t!("error.parse-timestamp"),
            ParseSysInfo => t!("error.parse-sys-info"),
            Wifi => t!("error.wifi"),
            Journald => t!("error.journald"),
            DiskManagement => t!("error.disk-management"),
            OpenSsl => t!("error.openssl"),
            PasswordHashGeneration => t!("error.password-hash-generation"),
            DiagnosticMode => t!("error.diagnostic-mode"),
            ParseDbField => t!("error.parse-db-field"),
            Duplicate => t!("error.duplicate"),
            MultipleErrors => t!("error.multiple-errors"),
            Incoherent => t!("error.incoherent"),
            InvalidBackupTargetId => t!("error.invalid-backup-target-id"),
            ProductKeyMismatch => t!("error.product-key-mismatch"),
            LanPortConflict => t!("error.lan-port-conflict"),
            ServiceRuntime => t!("error.service-runtime"),
            Pem => t!("error.pem"),
            TLSInit => t!("error.tls-init"),
            Ascii => t!("error.ascii"),
            MissingHeader => t!("error.missing-header"),
            Grub => t!("error.grub"),
            Systemd => t!("error.systemd"),
            OpenSsh => t!("error.openssh"),
            Zram => t!("error.zram"),
            Lshw => t!("error.lshw"),
            CpuSettings => t!("error.cpu-settings"),
            Firmware => t!("error.firmware"),
            Timeout => t!("error.timeout"),
            Lxc => t!("error.lxc"),
            Cancelled => t!("error.cancelled"),
            Git => t!("error.git"),
            DBus => t!("error.dbus"),
            InstallFailed => t!("error.install-failed"),
            UpdateFailed => t!("error.update-failed"),
            Smtp => t!("error.smtp"),
            SetSysInfo => t!("error.set-sys-info"),
            Bios => t!("error.bios"),
            BackupPasswordMismatch => t!("error.backup-password-mismatch"),
        }
        .to_string()
    }
}
impl Display for ErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.as_str())
    }
}

pub struct Error {
    pub source: color_eyre::eyre::Error,
    pub debug: Option<color_eyre::eyre::Error>,
    pub kind: ErrorKind,
    pub info: Value,
    pub task: Option<JoinHandle<()>>,
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", &self.kind.as_str(), self.display_src())
    }
}
impl Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", &self.kind.as_str(), self.display_dbg())
    }
}
impl Error {
    pub fn new<E: Into<color_eyre::eyre::Error> + std::fmt::Debug + 'static>(
        source: E,
        kind: ErrorKind,
    ) -> Self {
        let debug = (std::any::TypeId::of::<E>()
            == std::any::TypeId::of::<color_eyre::eyre::Error>())
        .then(|| eyre!("{source:?}"));
        Error {
            source: source.into(),
            debug,
            kind,
            info: Value::Null,
            task: None,
        }
    }
    pub fn clone_output(&self) -> Self {
        Error {
            source: eyre!("{:#}", self.source),
            debug: Some(
                self.debug
                    .as_ref()
                    .map(|e| eyre!("{e}"))
                    .unwrap_or_else(|| eyre!("{:?}", self.source)),
            ),
            kind: self.kind,
            info: self.info.clone(),
            task: None,
        }
    }
    pub fn with_task(mut self, task: JoinHandle<()>) -> Self {
        self.task = Some(task);
        self
    }
    pub fn with_info(mut self, info: Value) -> Self {
        self.info = info;
        self
    }
    pub async fn wait(mut self) -> Self {
        if let Some(task) = &mut self.task {
            task.await.log_err();
        }
        self.task.take();
        self
    }

    pub fn display_src(&self) -> impl Display {
        struct D<'a>(&'a Error);
        impl<'a> Display for D<'a> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{:#}", self.0.source)
            }
        }
        D(self)
    }

    pub fn display_dbg(&self) -> impl Display {
        struct D<'a>(&'a Error);
        impl<'a> Display for D<'a> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                if let Some(debug) = &self.0.debug {
                    write!(f, "{}", debug)
                } else {
                    write!(f, "{:?}", self.0.source)
                }
            }
        }
        D(self)
    }
}
impl axum::response::IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        let mut res = axum::Json(RpcError::from(self)).into_response();
        *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
        res
    }
}
impl From<std::convert::Infallible> for Error {
    fn from(value: std::convert::Infallible) -> Self {
        match value {}
    }
}
impl From<InvalidId> for Error {
    fn from(err: InvalidId) -> Self {
        Error::new(err, ErrorKind::InvalidId)
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
impl From<exver::ParseError> for Error {
    fn from(e: exver::ParseError) -> Self {
        Error::new(e, ErrorKind::ParseVersion)
    }
}
impl From<rpc_toolkit::url::ParseError> for Error {
    fn from(e: rpc_toolkit::url::ParseError) -> Self {
        Error::new(e, ErrorKind::ParseUrl)
    }
}
impl From<std::num::ParseIntError> for Error {
    fn from(e: std::num::ParseIntError) -> Self {
        Error::new(e, ErrorKind::ParseNumber)
    }
}
impl From<std::num::ParseFloatError> for Error {
    fn from(e: std::num::ParseFloatError) -> Self {
        Error::new(e, ErrorKind::ParseNumber)
    }
}
impl From<patch_db::Error> for Error {
    fn from(e: patch_db::Error) -> Self {
        Error::new(e, ErrorKind::Database)
    }
}
impl From<ed25519_dalek::SignatureError> for Error {
    fn from(e: ed25519_dalek::SignatureError) -> Self {
        Error::new(e, ErrorKind::InvalidSignature)
    }
}
impl From<std::net::AddrParseError> for Error {
    fn from(e: std::net::AddrParseError) -> Self {
        Error::new(e, ErrorKind::ParseNetAddress)
    }
}
impl From<ipnet::AddrParseError> for Error {
    fn from(e: ipnet::AddrParseError) -> Self {
        Error::new(e, ErrorKind::ParseNetAddress)
    }
}
impl From<openssl::error::ErrorStack> for Error {
    fn from(e: openssl::error::ErrorStack) -> Self {
        Error::new(eyre!("{}", e), ErrorKind::OpenSsl)
    }
}
impl From<mbrman::Error> for Error {
    fn from(e: mbrman::Error) -> Self {
        Error::new(e, ErrorKind::DiskManagement)
    }
}
impl From<gpt::GptError> for Error {
    fn from(e: gpt::GptError) -> Self {
        Error::new(e, ErrorKind::DiskManagement)
    }
}
impl From<gpt::mbr::MBRError> for Error {
    fn from(e: gpt::mbr::MBRError) -> Self {
        Error::new(e, ErrorKind::DiskManagement)
    }
}
impl From<InvalidUri> for Error {
    fn from(e: InvalidUri) -> Self {
        Error::new(eyre!("{}", e), ErrorKind::ParseUrl)
    }
}
impl From<ssh_key::Error> for Error {
    fn from(e: ssh_key::Error) -> Self {
        Error::new(e, ErrorKind::OpenSsh)
    }
}
impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        let kind = match e {
            _ if e.is_builder() => ErrorKind::ParseUrl,
            _ if e.is_decode() => ErrorKind::Deserialization,
            _ => ErrorKind::Network,
        };
        Error::new(e, kind)
    }
}
impl From<zbus::Error> for Error {
    fn from(e: zbus::Error) -> Self {
        Error::new(e, ErrorKind::DBus)
    }
}
impl From<rustls::Error> for Error {
    fn from(e: rustls::Error) -> Self {
        Error::new(e, ErrorKind::OpenSsl)
    }
}
impl From<lettre::error::Error> for Error {
    fn from(e: lettre::error::Error) -> Self {
        Error::new(e, ErrorKind::Smtp)
    }
}
impl From<lettre::transport::smtp::Error> for Error {
    fn from(e: lettre::transport::smtp::Error) -> Self {
        Error::new(e, ErrorKind::Smtp)
    }
}
impl From<lettre::address::AddressError> for Error {
    fn from(e: lettre::address::AddressError) -> Self {
        Error::new(e, ErrorKind::Smtp)
    }
}
impl From<hyper::Error> for Error {
    fn from(e: hyper::Error) -> Self {
        Error::new(e, ErrorKind::Network)
    }
}
impl From<patch_db::value::Error> for Error {
    fn from(value: patch_db::value::Error) -> Self {
        match value.kind {
            patch_db::value::ErrorKind::Serialization => {
                Error::new(value.source, ErrorKind::Serialization)
            }
            patch_db::value::ErrorKind::Deserialization => {
                Error::new(value.source, ErrorKind::Deserialization)
            }
        }
    }
}

#[derive(Clone, Deserialize, Serialize, TS)]
pub struct ErrorData {
    pub details: String,
    pub debug: String,
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
        Display::fmt(&self.debug, f)
    }
}
impl std::error::Error for ErrorData {}
impl From<Error> for ErrorData {
    fn from(value: Error) -> Self {
        let details = value.display_src().to_string();
        let debug = value.display_dbg().to_string();
        Self {
            details,
            debug,
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
            debug: value
                .data
                .as_ref()
                .and_then(|d| {
                    d.as_object()
                        .and_then(|d| {
                            d.get("debug")
                                .and_then(|d| d.as_str().map(|s| s.to_owned()))
                        })
                        .or_else(|| d.as_str().map(|s| s.to_owned()))
                })
                .unwrap_or_else(|| value.message.clone().into_owned()),
            info: to_value(
                &value
                    .data
                    .as_ref()
                    .and_then(|d| d.as_object().and_then(|d| d.get("info"))),
            )
            .unwrap_or_default(),
        }
    }
}

impl From<Error> for RpcError {
    fn from(e: Error) -> Self {
        let kind = e.kind;
        let data = ErrorData::from(e);
        RpcError {
            code: kind as i32,
            message: kind.as_str().into(),
            data: Some(match serde_json::to_value(&data) {
                Ok(a) => a,
                Err(e) => {
                    tracing::warn!("Error serializing ErrorData object: {}", e);
                    serde_json::Value::Null
                }
            }),
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
impl std::fmt::Display for ErrorCollection {
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
            let debug = (std::any::TypeId::of::<E>()
                == std::any::TypeId::of::<color_eyre::eyre::Error>())
            .then(|| eyre!("{ctx}: {e:?}"));
            let source = color_eyre::eyre::Error::from(e);
            let with_ctx = format!("{ctx}: {source}");
            let source = source.wrap_err(with_ctx);
            Error {
                kind,
                source,
                debug,
                info: Value::Null,
                task: None,
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
impl<T> ResultExt<T, Error> for Result<T, Error> {
    fn with_kind(self, kind: ErrorKind) -> Result<T, Error> {
        self.map_err(|e| Error { kind, ..e })
    }

    fn with_ctx<F: FnOnce(&Error) -> (ErrorKind, D), D: Display>(self, f: F) -> Result<T, Error> {
        self.map_err(|e| {
            let (kind, ctx) = f(&e);
            let ctx = InternedString::from_display(&ctx);
            let source = e.source;
            let source = source.wrap_err(ctx.clone());
            let debug = e.debug.map(|e| e.wrap_err(ctx));
            Error {
                kind,
                source,
                debug,
                ..e
            }
        })
    }

    fn log_err(self) -> Option<T> {
        match self {
            Ok(a) => Some(a),
            Err(e) => {
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

#[macro_export]
macro_rules! ensure_code {
    ($x:expr, $c:expr, $fmt:expr $(, $arg:expr)*) => {
        if !($x) {
            return Err(Error::new(color_eyre::eyre::eyre!($fmt, $($arg, )*), $c));
        }
    };
}
