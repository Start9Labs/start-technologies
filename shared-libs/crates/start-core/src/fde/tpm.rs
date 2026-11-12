use std::io::Cursor;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use openssl::hash::MessageDigest;
use openssl::pkey::{PKey, Private, Public};
use openssl::sign::{Signer, Verifier};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use zeroize::Zeroizing;

use crate::prelude::*;
use crate::util::Invoke;
use crate::util::io::{TmpDir, write_file_private_atomic};

const PCR_REGISTERS: [u8; 7] = [0, 2, 4, 7, 11, 14, 15];

fn pcr_selection() -> String {
    format!(
        "sha256:{}",
        PCR_REGISTERS.map(|pcr| pcr.to_string()).join(",")
    )
}
const POLICY_REF: &[u8] = b"StartOS FDE bootstrap policy v1";
const PRIMARY_TEMPLATE: &str = "ecc256:null:aes128cfb";
const PRIMARY_ATTRIBUTES: &str =
    "restricted|decrypt|fixedtpm|fixedparent|sensitivedataorigin|userwithauth";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SealedKey {
    pub public: Vec<u8>,
    pub private: Vec<u8>,
}

/// Authorizes a TPM PCR policy, not an overlay manifest.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Authorization {
    pub policy: [u8; 32],
    pub signature: Vec<u8>,
}

/// Firmware, option ROMs, loaded applications, Secure Boot, UKI, MOK policy and verified filesystems.
/// The authority must authorize the direct firmware → shim → UKI route, excluding command interpreters.
#[derive(Clone, Debug)]
pub struct BootMeasurements {
    pub firmware: [u8; 32],
    pub option_roms: [u8; 32],
    pub applications: [u8; 32],
    pub secure_boot: [u8; 32],
    pub uki: [u8; 32],
    pub shim_policy: [u8; 32],
    pub files: [u8; 32],
}

impl BootMeasurements {
    fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() != PCR_REGISTERS.len() * 32 {
            return Err(Error::new(
                eyre!("{}", t!("fde.tpm.invalid-policy")),
                ErrorKind::Bios,
            ));
        }
        let values: [[u8; 32]; PCR_REGISTERS.len()] = bytes
            .chunks_exact(32)
            .map(|chunk| chunk.try_into().unwrap())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let [
            firmware,
            option_roms,
            applications,
            secure_boot,
            uki,
            shim_policy,
            files,
        ] = values;
        Ok(Self {
            firmware,
            option_roms,
            applications,
            secure_boot,
            uki,
            shim_policy,
            files,
        })
    }

    fn bytes(&self) -> Vec<u8> {
        [
            &self.firmware[..],
            &self.option_roms[..],
            &self.applications[..],
            &self.secure_boot[..],
            &self.uki[..],
            &self.shim_policy[..],
            &self.files[..],
        ]
        .concat()
    }
}

/// The caller verifies stable rootfs and overlay content before extending PCR 15.
pub fn filesystem_event(rootfs: &[u8; 32], overlay: &[u8; 32]) -> [u8; 32] {
    openssl::sha::sha256(
        &[
            &b"StartOS FDE filesystems v1\0"[..],
            rootfs.as_slice(),
            overlay.as_slice(),
        ]
        .concat(),
    )
}

pub fn filesystem_pcr(event: &[u8; 32]) -> [u8; 32] {
    openssl::sha::sha256(&[&[0; 32][..], event.as_slice()].concat())
}

struct Workspace(TmpDir);

impl Workspace {
    async fn new() -> Result<Self, Error> {
        let directory = TmpDir::new().await?;
        tokio::fs::set_permissions(&*directory, std::fs::Permissions::from_mode(0o700)).await?;
        Ok(Self(directory))
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn command(&self, executable: &str) -> Command {
        let mut command = Command::new(executable);
        command
            .current_dir(self.path())
            .env_remove("TPM2TOOLS_AUTOFLUSH");
        command
    }

    async fn run(&self, executable: &str, arguments: &[&str]) -> Result<Vec<u8>, Error> {
        self.command(executable)
            .args(arguments)
            .invoke(ErrorKind::Bios)
            .await
    }

    async fn write(&self, name: &str, bytes: &[u8]) -> Result<(), Error> {
        write_file_private_atomic(self.path().join(name), bytes).await
    }

    async fn read(&self, name: &str) -> Result<Vec<u8>, Error> {
        Ok(tokio::fs::read(self.path().join(name)).await?)
    }

    async fn load_authority(&self, authority: &PKey<Public>) -> Result<(), Error> {
        self.write("authority.pem", &authority.public_key_to_pem()?)
            .await?;
        self.run(
            "tpm2_loadexternal",
            &[
                "-C",
                "o",
                "-G",
                "ecc",
                "-g",
                "sha256",
                "-u",
                "authority.pem",
                "-c",
                "authority.ctx",
                "-n",
                "authority.name",
            ],
        )
        .await?;
        self.write("policy.ref", POLICY_REF).await
    }

    async fn create_primary(&self) -> Result<(), Error> {
        self.run(
            "tpm2_createprimary",
            &[
                "-C",
                "o",
                "-g",
                "sha256",
                "-G",
                PRIMARY_TEMPLATE,
                "-a",
                PRIMARY_ATTRIBUTES,
                "-c",
                "primary.ctx",
            ],
        )
        .await?;
        Ok(())
    }

    async fn restrict_to_unseal(&self, session: &str, output: &str) -> Result<(), Error> {
        self.run(
            "tpm2_policycommandcode",
            &["-S", session, "-L", output, "TPM2_CC_Unseal"],
        )
        .await?;
        Ok(())
    }

    async fn flush(&self, context: &str) {
        self.run("tpm2_flushcontext", &[context]).await.log_err();
    }
}

fn public_key(key: &PKey<Private>) -> Result<PKey<Public>, Error> {
    Ok(PKey::public_key_from_der(&key.public_key_to_der()?)?)
}

fn authorization_message(policy: &[u8; 32]) -> Vec<u8> {
    [policy.as_slice(), POLICY_REF].concat()
}

pub async fn authorize(
    measurements: &BootMeasurements,
    authority: &PKey<Private>,
) -> Result<Authorization, Error> {
    let workspace = Workspace::new().await?;
    workspace.write("pcrs.bin", &measurements.bytes()).await?;
    workspace
        .run(
            "tpm2_createpolicy",
            &[
                "--policy-pcr",
                "-g",
                "sha256",
                "-l",
                &pcr_selection(),
                "-f",
                "pcrs.bin",
                "-L",
                "approved.policy",
            ],
        )
        .await?;
    let policy: [u8; 32] = workspace
        .read("approved.policy")
        .await?
        .try_into()
        .map_err(|_| Error::new(eyre!("{}", t!("fde.tpm.invalid-policy")), ErrorKind::Bios))?;
    let mut signer = Signer::new(MessageDigest::sha256(), authority)?;
    let signature = signer.sign_oneshot_to_vec(&authorization_message(&policy))?;
    Ok(Authorization { policy, signature })
}

pub fn verify_authorization(
    authorization: &Authorization,
    authority: &PKey<Public>,
) -> Result<bool, Error> {
    let mut verifier = Verifier::new(MessageDigest::sha256(), authority)?;
    Ok(verifier.verify_oneshot(
        &authorization.signature,
        &authorization_message(&authorization.policy),
    )?)
}

/// Creates a TPM-bound object with policy-only authorization and encrypted input.
pub async fn seal(key: &[u8; 32], authority: &PKey<Private>) -> Result<SealedKey, Error> {
    let workspace = Workspace::new().await?;
    workspace.load_authority(&public_key(authority)?).await?;
    workspace
        .run(
            "tpm2_startauthsession",
            &["-g", "sha256", "-S", "trial.ctx"],
        )
        .await?;
    let trial = async {
        workspace
            .run(
                "tpm2_policyauthorize",
                &[
                    "-S",
                    "trial.ctx",
                    "-n",
                    "authority.name",
                    "-q",
                    "policy.ref",
                    "-L",
                    "authorized.policy",
                ],
            )
            .await?;
        workspace
            .restrict_to_unseal("trial.ctx", "sealed.policy")
            .await
    }
    .await;
    workspace.flush("trial.ctx").await;
    trial?;
    workspace.create_primary().await?;
    workspace
        .run(
            "tpm2_startauthsession",
            &[
                "--hmac-session",
                "-g",
                "sha256",
                "-c",
                "primary.ctx",
                "-S",
                "parent.ctx",
            ],
        )
        .await?;
    let result = workspace
        .command("tpm2_create")
        .args([
            "-C",
            "primary.ctx",
            "-P",
            "session:parent.ctx",
            "-g",
            "sha256",
            "-L",
            "sealed.policy",
            "-a",
            "fixedtpm|fixedparent|adminwithpolicy",
            "-i",
            "-",
            "-u",
            "sealed.pub",
            "-r",
            "sealed.priv",
        ])
        .input(Some(&mut Cursor::new(key)))
        .invoke(ErrorKind::Bios)
        .await;
    workspace.flush("parent.ctx").await;
    workspace.flush("primary.ctx").await;
    workspace.flush("authority.ctx").await;
    result?;
    Ok(SealedKey {
        public: workspace.read("sealed.pub").await?,
        private: workspace.read("sealed.priv").await?,
    })
}

/// Releases bootstrap material only; no data-volume credential is sealed here.
pub async fn unseal(
    sealed: &SealedKey,
    authorization: &Authorization,
    authority: &PKey<Public>,
) -> Result<Zeroizing<[u8; 32]>, Error> {
    if !verify_authorization(authorization, authority)? {
        return Err(Error::new(
            eyre!("{}", t!("fde.tpm.invalid-authorization")),
            ErrorKind::InvalidSignature,
        ));
    }
    let workspace = Workspace::new().await?;
    workspace.write("sealed.pub", &sealed.public).await?;
    workspace.write("sealed.priv", &sealed.private).await?;
    workspace
        .write("approved.policy", &authorization.policy)
        .await?;
    workspace
        .write("authorization.sig", &authorization.signature)
        .await?;
    workspace
        .write(
            "authorization.message",
            &authorization_message(&authorization.policy),
        )
        .await?;
    workspace.load_authority(authority).await?;
    workspace
        .run(
            "tpm2_verifysignature",
            &[
                "-c",
                "authority.ctx",
                "-g",
                "sha256",
                "-m",
                "authorization.message",
                "-s",
                "authorization.sig",
                "-f",
                "ecdsa",
                "-t",
                "verification.ticket",
            ],
        )
        .await?;
    workspace.create_primary().await?;
    workspace
        .run(
            "tpm2_load",
            &[
                "-C",
                "primary.ctx",
                "-u",
                "sealed.pub",
                "-r",
                "sealed.priv",
                "-c",
                "sealed.ctx",
            ],
        )
        .await?;
    workspace
        .run(
            "tpm2_startauthsession",
            &[
                "--policy-session",
                "-g",
                "sha256",
                "-c",
                "primary.ctx",
                "-S",
                "policy.ctx",
            ],
        )
        .await?;
    let result = async {
        workspace
            .run(
                "tpm2_policypcr",
                &["-S", "policy.ctx", "-l", &pcr_selection()],
            )
            .await?;
        workspace
            .run(
                "tpm2_policyauthorize",
                &[
                    "-S",
                    "policy.ctx",
                    "-i",
                    "approved.policy",
                    "-n",
                    "authority.name",
                    "-t",
                    "verification.ticket",
                    "-q",
                    "policy.ref",
                ],
            )
            .await?;
        workspace
            .restrict_to_unseal("policy.ctx", "runtime.policy")
            .await?;
        let bytes = Zeroizing::new(
            workspace
                .run(
                    "tpm2_unseal",
                    &["-c", "sealed.ctx", "-p", "session:policy.ctx"],
                )
                .await?,
        );
        let mut key = Zeroizing::new([0; 32]);
        if bytes.len() != key.len() {
            return Err(Error::new(
                eyre!("{}", t!("fde.tpm.invalid-key")),
                ErrorKind::Bios,
            ));
        }
        key.copy_from_slice(&bytes);
        Ok(key)
    }
    .await;
    workspace.flush("policy.ctx").await;
    workspace.flush("sealed.ctx").await;
    workspace.flush("primary.ctx").await;
    workspace.flush("authority.ctx").await;
    result
}

pub async fn read_measurements() -> Result<BootMeasurements, Error> {
    let workspace = Workspace::new().await?;
    workspace
        .run("tpm2_pcrread", &["-o", "pcrs.bin", &pcr_selection()])
        .await?;
    let bytes = workspace.read("pcrs.bin").await?;
    BootMeasurements::from_bytes(&bytes)
}

pub async fn measure_filesystems(event: &[u8; 32]) -> Result<(), Error> {
    let workspace = Workspace::new().await?;
    if read_measurements().await?.files != [0; 32] {
        return Err(Error::new(
            eyre!("{}", t!("fde.tpm.pcr-already-used")),
            ErrorKind::Bios,
        ));
    }
    workspace
        .run(
            "tpm2_pcrextend",
            &[&format!("15:sha256={}", hex::encode(event))],
        )
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measurement_file_binds_loaded_applications_before_secure_boot() {
        let bytes = [0u8, 2, 4, 7, 11, 14, 15].map(|pcr| [pcr; 32]).concat();
        let measurements = BootMeasurements::from_bytes(&bytes).unwrap();
        assert_eq!(measurements.applications, [4; 32]);
        assert_eq!(measurements.secure_boot, [7; 32]);
        assert_eq!(measurements.uki, [11; 32]);
        assert_eq!(measurements.shim_policy, [14; 32]);
        assert_eq!(measurements.files, [15; 32]);
        assert_eq!(measurements.bytes(), bytes);
        assert!(BootMeasurements::from_bytes(&bytes[..bytes.len() - 1]).is_err());
        let mut extra = bytes;
        extra.push(0);
        assert!(BootMeasurements::from_bytes(&extra).is_err());
    }

    #[test]
    fn rootfs_and_overlay_both_change_the_filesystem_measurement() {
        let rootfs = [1; 32];
        let overlay = [2; 32];
        let event = filesystem_event(&rootfs, &overlay);
        assert_ne!(event, filesystem_event(&[3; 32], &overlay));
        assert_ne!(event, filesystem_event(&rootfs, &[3; 32]));
        assert_ne!(event, filesystem_event(&overlay, &rootfs));
    }

    #[test]
    fn policy_authorization_is_bound_to_its_digest_and_authority() {
        let key = crate::net::ssl::gen_nistp256().unwrap();
        let policy = [9; 32];
        let signature = Signer::new(MessageDigest::sha256(), &key)
            .unwrap()
            .sign_oneshot_to_vec(&authorization_message(&policy))
            .unwrap();
        let mut authorization = Authorization { policy, signature };
        assert!(verify_authorization(&authorization, &public_key(&key).unwrap()).unwrap());
        authorization.policy[0] ^= 1;
        assert!(!verify_authorization(&authorization, &public_key(&key).unwrap()).unwrap());
        authorization.policy[0] ^= 1;
        let other_key = crate::net::ssl::gen_nistp256().unwrap();
        assert!(!verify_authorization(&authorization, &public_key(&other_key).unwrap()).unwrap());
    }

    #[tokio::test]
    #[ignore = "requires an exclusively owned software TPM and tpm2-tools"]
    async fn software_tpm_unseals_only_the_authorized_boot_state() {
        let authority = crate::net::ssl::gen_nistp256().unwrap();
        let key = super::super::bootstrap::generate_key().unwrap();
        let measurements = read_measurements().await.unwrap();
        let mut authorization = authorize(&measurements, &authority).await.unwrap();
        let sealed = seal(&key, &authority).await.unwrap();
        let authority_public = public_key(&authority).unwrap();
        assert_eq!(
            unseal(&sealed, &authorization, &authority_public)
                .await
                .unwrap()
                .as_slice(),
            key.as_slice(),
        );
        let workspace = Workspace::new().await.unwrap();
        for register in PCR_REGISTERS {
            workspace
                .run(
                    "tpm2_pcrextend",
                    &[&format!("{register}:sha256={}", "01".repeat(32))],
                )
                .await
                .unwrap();
            assert!(
                unseal(&sealed, &authorization, &authority_public)
                    .await
                    .is_err(),
                "changed PCR {register} released the bootstrap key",
            );
            authorization = authorize(&read_measurements().await.unwrap(), &authority)
                .await
                .unwrap();
            assert_eq!(
                unseal(&sealed, &authorization, &authority_public)
                    .await
                    .unwrap()
                    .as_slice(),
                key.as_slice(),
            );
        }
        let other = crate::net::ssl::gen_nistp256().unwrap();
        let other_authorization = authorize(&read_measurements().await.unwrap(), &other)
            .await
            .unwrap();
        assert!(
            unseal(&sealed, &other_authorization, &public_key(&other).unwrap())
                .await
                .is_err()
        );
        workspace.write("sealed.pub", &sealed.public).await.unwrap();
        workspace
            .write("sealed.priv", &sealed.private)
            .await
            .unwrap();
        workspace.create_primary().await.unwrap();
        workspace
            .run(
                "tpm2_load",
                &[
                    "-C",
                    "primary.ctx",
                    "-u",
                    "sealed.pub",
                    "-r",
                    "sealed.priv",
                    "-c",
                    "sealed.ctx",
                ],
            )
            .await
            .unwrap();
        assert!(
            workspace
                .run("tpm2_unseal", &["-c", "sealed.ctx"])
                .await
                .is_err()
        );
        workspace.flush("sealed.ctx").await;
        workspace.flush("primary.ctx").await;
    }
}
