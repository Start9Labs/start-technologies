use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};

use crate::prelude::*;

const JSON_ENV_PREFIX: &str = "JSON_ENV_";
const JSON_ENV_CHUNK_BYTES: usize = 64 * crate::CAP_1_KiB;

/// Environment overrides; `None` removes a variable.
pub type Environment = BTreeMap<String, Option<String>>;

/// Splits an environment assignment; a bare name removes the variable.
pub fn parse_assignment(line: &str) -> (&str, Option<&str>) {
    line.split_once('=')
        .map_or((line, None), |(key, value)| (key, Some(value)))
}

fn chunk_suffix(name: &OsStr) -> Option<&str> {
    name.to_str()?
        .strip_prefix(JSON_ENV_PREFIX)
        .filter(|suffix| !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()))
}

/// Whether a variable belongs to the numbered JSON environment transport.
pub fn is_json_env_chunk(name: &OsStr) -> bool {
    chunk_suffix(name).is_some()
}

/// Produces UTF-8 chunks small enough for Linux's per-string exec limit.
pub fn json_env_chunks(mut json: &str) -> impl Iterator<Item = (String, &str)> {
    let mut index = 0;
    std::iter::from_fn(move || {
        if json.is_empty() {
            return None;
        }
        let (chunk, rest) = json.split_at(json.floor_char_boundary(JSON_ENV_CHUNK_BYTES));
        json = rest;
        let name = format!("{JSON_ENV_PREFIX}{index}");
        index += 1;
        Some((name, chunk))
    })
}

/// Reads a JSON object from contiguous numbered chunks, starting at zero.
pub fn parse_json_env(
    variables: impl IntoIterator<Item = (OsString, OsString)>,
) -> Result<Environment, Error> {
    let mut chunks = BTreeMap::new();
    for (name, value) in variables {
        if let Some(suffix) = chunk_suffix(&name) {
            let index = suffix.parse::<usize>().map_err(|_| invalid_json_env())?;
            if suffix != index.to_string() {
                return Err(invalid_json_env());
            }
            chunks.insert(index, value);
        }
    }
    if chunks.is_empty() {
        return Ok(Environment::new());
    }
    let mut json = Vec::new();
    for (expected, (index, chunk)) in chunks.into_iter().enumerate() {
        if index != expected {
            return Err(invalid_json_env());
        }
        json.extend_from_slice(chunk.as_encoded_bytes());
    }
    serde_json::from_slice(&json).map_err(|_| invalid_json_env())
}

fn invalid_json_env() -> Error {
    Error::new(
        eyre!("{}", t!("util.env.invalid-json-env")),
        ErrorKind::Deserialization,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbered_chunks_preserve_large_unicode_environments_in_numeric_order() {
        let environment: Environment = (0..16)
            .map(|i| (format!("VALUE_{i}"), Some("🦀".repeat(11_000))))
            .chain([
                ("REMOVED".into(), None),
                ("EMPTY".into(), Some(String::new())),
            ])
            .collect();
        let json = serde_json::to_string(&environment).unwrap();
        let chunks: Vec<_> = json_env_chunks(&json)
            .map(|(name, value)| (OsString::from(name), OsString::from(value)))
            .collect();
        assert!(chunks.len() > 10);
        assert!(chunks.iter().all(|(_, value)| value.len() <= 64 * 1024));
        assert_eq!(
            parse_json_env(chunks.iter().cloned().rev()).unwrap(),
            environment
        );
        assert!(
            std::process::Command::new("/bin/true")
                .env_clear()
                .envs(chunks)
                .status()
                .unwrap()
                .success()
        );
    }

    #[test]
    fn empty_transport_and_unrelated_variables_do_not_add_overrides() {
        assert!(parse_json_env([]).unwrap().is_empty());
        assert!(
            parse_json_env([
                ("JSON_ENV_OTHER".into(), "ignored".into()),
                ("PATH".into(), "/bin".into()),
            ])
            .unwrap()
            .is_empty()
        );
    }

    #[test]
    fn invalid_chunks_do_not_echo_secrets() {
        for entries in [
            vec![("JSON_ENV_0", "\"secret-value\"")],
            vec![("JSON_ENV_0", "{\"TOKEN\":[\"secret-value\"]}")],
            vec![("JSON_ENV_0", "{")],
            vec![("JSON_ENV_0", "null")],
            vec![("JSON_ENV_0", "[]")],
            vec![("JSON_ENV_0", "{\"PORT\":42}")],
            vec![("JSON_ENV_0", "")],
            vec![("JSON_ENV_1", "{}")],
            vec![("JSON_ENV_0", "{}"), ("JSON_ENV_2", "secret-value")],
        ] {
            let error =
                parse_json_env(entries.into_iter().map(|(k, v)| (k.into(), v.into()))).unwrap_err();
            assert_eq!(error.kind, ErrorKind::Deserialization);
            assert!(!format!("{error:?}").contains("secret-value"));
        }
    }
}
