use crate::prelude::*;
use clap::{ArgMatches, CommandFactory, FromArgMatches};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::any::type_name;
use std::io::Write as _;
use std::process::Command;

// Re-export startos utilities — same behavior, maintained in one place.
pub use startos::util::serde::{DisplaySerializable, HandlerExtSerde};

/// A clap-compatible wrapper that reads the inner value as JSON from stdin
/// instead of from CLI args. Used for commands that take complex structured
/// input (entire profile objects, etc.).
///
/// This is a thin wrapper around `startos::util::serde::StdinDeserializable`
/// that adds a `CommandFactory` impl so it can be used directly as a handler
/// `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
/// which is how startos uses its version).
#[derive(Deserialize, Serialize, Clone)]
pub struct DeserializeStdin<T>(pub T);

impl<T: Default> Default for DeserializeStdin<T> {
    fn default() -> Self {
        Self(T::default())
    }
}

impl<T: DeserializeOwned> FromArgMatches for DeserializeStdin<T> {
    fn from_arg_matches(matches: &ArgMatches) -> Result<Self, clap::Error> {
        startos::util::serde::StdinDeserializable::<T>::from_arg_matches(matches).map(|s| Self(s.0))
    }
    fn update_from_arg_matches(&mut self, matches: &ArgMatches) -> Result<(), clap::Error> {
        *self = Self::from_arg_matches(matches)?;
        Ok(())
    }
}

impl<T: DeserializeOwned> CommandFactory for DeserializeStdin<T> {
    fn command() -> clap::Command {
        use clap::Args;
        startos::util::serde::StdinDeserializable::<T>::augment_args(
            clap::Command::new(env!("CARGO_PKG_NAME"))
                .after_help(format!("will read {} from stdin", type_name::<T>())),
        )
    }
    fn command_for_update() -> clap::Command {
        Self::command()
    }
}

/// Opens the given data in $EDITOR, allowing interactive editing.
/// Returns the modified data after the editor closes.
pub fn edit_in_editor<T>(data: &T) -> Result<T, Error>
where
    T: Serialize + DeserializeOwned,
{
    let mut temp_file = tempfile::Builder::new().suffix(".json").tempfile()?;

    serde_json::to_writer_pretty(&mut temp_file, data)
        .map_err(|e| Error::new(eyre!("Failed to write JSON: {}", e), ErrorKind::Serialization))?;
    temp_file.flush()?;

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
    let temp_path = temp_file.path().to_owned();

    let status = Command::new(&editor)
        .arg(&temp_path)
        .status()
        .map_err(|e| Error::new(eyre!("starting editor {editor}: {e}"), ErrorKind::Filesystem))?;

    if !status.success() {
        return Err(Error::new(eyre!("Editor exited with non-zero status"), ErrorKind::Filesystem));
    }

    let content = std::fs::read_to_string(&temp_path)?;

    serde_json::from_str(&content)
        .map_err(|e| Error::new(eyre!("Failed to parse JSON: {}", e), ErrorKind::Deserialization))
}
