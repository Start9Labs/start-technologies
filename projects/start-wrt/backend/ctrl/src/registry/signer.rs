use serde::{Deserialize, Serialize};

use crate::sign::commitment::Digestable;
use crate::sign::{AnySignature, AnyVerifyingKey, SignatureScheme};
use crate::Error;

/// Declarative signature acceptance policy, mirroring start-os's `AcceptSigners`.
///
/// Validates signatures against a composable tree of requirements:
/// - `Signer(key)` — require a specific signer
/// - `All(vec)` — require ALL sub-conditions
/// - `Any(vec)` — require at least one sub-condition
/// - `Accepted` — already satisfied
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
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

    pub fn accepted(&self) -> bool {
        matches!(self, Self::Accepted)
    }

    pub fn try_accept(self) -> Result<(), Error> {
        if self.accepted() {
            Ok(())
        } else {
            Err(Error::other("not all required signatures were satisfied"))
        }
    }

    /// Process a single signature, advancing the state machine.
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
                    // Match start-os: unconditionally transition to Accepted
                    // when the signer key matches. Verification errors propagate
                    // via `res` and the caller's `?` operator.
                    res = signer
                        .scheme()
                        .verify_commitment(signer, commitment, context, signature);
                    Self::Accepted
                } else {
                    Self::Signer(s)
                }
            }
            Self::All(mut v) => {
                // Match start-os: collect short-circuits on the first error,
                // so remaining sub-signers are not processed.
                res = v
                    .iter_mut()
                    .map(|item| item.process_signature(signer, commitment, context, signature))
                    .collect();
                if v.iter().all(|s| s.accepted()) {
                    Self::Accepted
                } else {
                    Self::All(v)
                }
            }
            Self::Any(mut v) => {
                let mut found = false;
                for item in v.iter_mut() {
                    match item.process_signature(signer, commitment, context, signature) {
                        Ok(()) if item.accepted() => {
                            found = true;
                            break;
                        }
                        Err(e) => {
                            res = Err(e);
                        }
                        _ => {}
                    }
                }
                if found {
                    Self::Accepted
                } else {
                    Self::Any(v)
                }
            }
        };
        *self = new;
        res
    }
}
