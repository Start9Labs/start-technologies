use std::collections::BTreeMap;

/// Environment overrides; `None` removes a variable.
pub type Environment = BTreeMap<String, Option<String>>;

/// Splits an environment assignment; a bare name removes the variable.
pub fn parse_assignment(line: &str) -> (&str, Option<&str>) {
    line.split_once('=')
        .map_or((line, None), |(key, value)| (key, Some(value)))
}
