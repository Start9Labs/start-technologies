use std::fmt::Display;
use std::io::Stdin;
use std::str::FromStr;

pub use clap;
use clap::ArgMatches;
pub use serde;

pub fn default_arg_parser<T>(arg: &str, _: &ArgMatches) -> Result<T, clap::Error>
where
    T: FromStr,
    T::Err: Display,
{
    arg.parse()
        .map_err(|e| clap::Error::raw(clap::error::ErrorKind::ValueValidation, e))
}

pub fn default_stdin_parser<T>(stdin: &mut Stdin, _: &ArgMatches) -> Result<T, clap::Error>
where
    T: FromStr,
    T::Err: Display,
{
    let mut s = String::new();
    stdin
        .read_line(&mut s)
        .map_err(|e| clap::Error::raw(clap::error::ErrorKind::Io, e))?;
    if let Some(s) = s.strip_suffix("\n") {
        s
    } else {
        &s
    }
    .parse()
    .map_err(|e| clap::Error::raw(clap::error::ErrorKind::ValueValidation, e))
}
