use std::collections::HashSet;
use std::str::FromStr;

use clap::builder::ValueParserFactory;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use url::Url;
use visit_rs::ts::TS;

use crate::prelude::*;
use crate::sign::commitment::Digestable;
use crate::sign::{AnySignature, AnyVerifyingKey, SignatureScheme};
use crate::util::FromStrParser;

#[derive(Debug, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
#[ts(export)]
pub struct SignerInfo {
    pub name: String,
    pub contact: Vec<ContactInfo>,
    pub keys: HashSet<AnyVerifyingKey>,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
// TODO: better types
pub enum ContactInfo {
    Email(String),
    Matrix(String),
    Website(#[ts(type = "string")] Url),
}
impl std::fmt::Display for ContactInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Email(e) => write!(f, "mailto:{e}"),
            Self::Matrix(m) => write!(f, "https://matrix.to/#/{m}"),
            Self::Website(w) => write!(f, "{w}"),
        }
    }
}
impl FromStr for ContactInfo {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(if let Some(s) = s.strip_prefix("mailto:") {
            Self::Email(s.to_owned())
        } else if let Some(s) = s.strip_prefix("https://matrix.to/#/") {
            Self::Matrix(s.to_owned())
        } else {
            Self::Website(s.parse()?)
        })
    }
}
impl ValueParserFactory for ContactInfo {
    type Parser = FromStrParser<Self>;
    fn value_parser() -> Self::Parser {
        Self::Parser::new()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AcceptSigners {
    #[serde(skip)]
    Accepted,
    Signer(AnyVerifyingKey),
    Any(Vec<AcceptSigners>),
    All(Vec<AcceptSigners>),
}
impl AcceptSigners {
    const fn null() -> Self {
        Self::Any(Vec::new())
    }
    pub fn flatten(self) -> Self {
        match self {
            Self::Any(mut s) | Self::All(mut s) if s.len() == 1 => s.swap_remove(0).flatten(),
            s => s,
        }
    }
    pub fn accepted(&self) -> bool {
        match self {
            Self::Accepted => true,
            _ => false,
        }
    }
    pub fn try_accept(self) -> Result<(), Error> {
        if self.accepted() {
            Ok(())
        } else {
            Err(Error::new(
                eyre!("{}", t!("registry.signer.not-accepted")),
                ErrorKind::InvalidSignature,
            ))
        }
    }
    pub fn process_signature(
        &mut self,
        signer: &AnyVerifyingKey,
        commitment: &impl Digestable,
        context: &str,
        signature: &AnySignature,
    ) -> Result<(), Error> {
        let mut res = Ok(());
        let new = match std::mem::replace(self, Self::null()) {
            Self::Accepted => Self::Accepted,
            Self::Signer(s) => {
                if &s == signer {
                    res = signer
                        .scheme()
                        .verify_commitment(signer, commitment, context, signature);
                    Self::Accepted
                } else {
                    Self::Signer(s)
                }
            }
            Self::All(mut s) => {
                res = s
                    .iter_mut()
                    .map(|s| s.process_signature(signer, commitment, context, signature))
                    .collect();
                if s.iter().all(|s| s.accepted()) {
                    Self::Accepted
                } else {
                    Self::All(s)
                }
            }
            Self::Any(mut s) => {
                match s
                    .iter_mut()
                    .map(|s| {
                        s.process_signature(signer, commitment, context, signature)?;
                        Ok(s)
                    })
                    .filter_ok(|s| s.accepted())
                    .next()
                {
                    Some(Ok(s)) => std::mem::replace(s, Self::null()),
                    Some(Err(e)) => {
                        res = Err(e);
                        Self::Any(s)
                    }
                    None => Self::Any(s),
                }
            }
        };
        *self = new;
        res
    }
}
