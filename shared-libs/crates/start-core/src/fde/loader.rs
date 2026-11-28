//! Direct-shim loader measurements for trusted enrollment and recovery.
//!
//! Capture requires an independently trusted running installation or recovery
//! environment and caller-authenticated artifacts for the entire current chain.
//! Event-log replay proves consistency, not trust in a compromised running host. Persist
//! the opaque baseline in the encrypted database, never in public boot config;
//! attestation failure must not trigger capture or automatic learning.
//!
//! Predictions authorize one exact firmware → shim → UKI route. The caller owns
//! approval of each boot/filesystem combination, stable artifacts through use,
//! unchanged firmware/shim/Secure Boot and PCR14 MOK policy, and exclusion of
//! unsigned stub sidecars. Changed MOK enrollment requires trusted re-enrollment,
//! not reuse of the old PCR14. The current legacy chain is validated then discarded;
//! future UKIs with an embedded-kernel PCR4 event remain unsupported. Local password
//! recovery remains independent of these measurements and the TPM.

use std::io::Cursor;
use std::path::Path;

use openssl::sha::sha256;
use openssl::x509::X509;
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use super::tpm::{self, extend_pcr};
use crate::prelude::*;
use crate::util::Invoke;
use crate::util::io::{TmpDir, write_file_atomic};
use crate::util::mok::{EfiSignature, EfiVariable, signature_entries};

const PCRLOCK: &str = "/usr/lib/systemd/systemd-pcrlock";
const EFIVARS: &str = "/sys/firmware/efi/efivars";
const SHIM_GUID: [u8; 16] = [
    0x50, 0xab, 0x5d, 0x60, 0x46, 0xe0, 0x00, 0x43, 0xab, 0xb6, 0x3d, 0xd8, 0x10, 0xdd, 0x8b, 0x23,
];
const GLOBAL_GUID: [u8; 16] = [
    0x61, 0xdf, 0xe4, 0x8b, 0xca, 0x93, 0xd2, 0x11, 0xaa, 0x0d, 0x00, 0xe0, 0x98, 0x03, 0x2b, 0x8c,
];
const DB_GUID: [u8; 16] = [
    0xcb, 0xb2, 0x19, 0xd7, 0x3a, 0x3d, 0x96, 0x45, 0xa3, 0xbc, 0xda, 0xd0, 0x0e, 0x67, 0x65, 0x6f,
];
const DB_FILE: &str = "db-d719b2cb-3d3a-4596-a3bc-dad00e67656f";
const MOK_FILE: &str = "MokListRT-605dab50-e046-4300-abb6-3dd810dd8b23";

/// Sanitized, encrypted-database-only enrollment evidence; fields are not an input API.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Baseline {
    version: u8,
    applications_prefix: Vec<[u8; 32]>,
    secure_boot_prefix: Vec<[u8; 32]>,
    firmware_db: [u8; 32],
}

/// A paired prediction, never independent PCR4/PCR7 alternatives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Measurements {
    pub applications: [u8; 32],
    pub secure_boot: [u8; 32],
}

#[derive(Deserialize)]
struct Digest {
    #[serde(rename = "hashAlg")]
    algorithm: String,
    digest: String,
}

#[derive(Deserialize)]
struct Record {
    pcr: u32,
    recnum: u64,
    digests: Vec<Digest>,
    content_type: Option<String>,
    content: Option<Content>,
}

#[derive(Deserialize)]
struct Content {
    event_type: String,
    event_data: String,
}

struct Event {
    number: u64,
    kind: String,
    data: Vec<u8>,
    digest: [u8; 32],
}

fn invalid() -> Error {
    Error::new(
        eyre!("{}", t!("fde.loader.unsupported-route")),
        ErrorKind::Bios,
    )
}

fn decode(value: &str) -> Result<Vec<u8>, Error> {
    hex::decode(value).map_err(|_| invalid())
}

fn digest(digests: &[Digest], algorithm: &str) -> Result<Vec<u8>, Error> {
    let mut matching = digests.iter().filter(|d| d.algorithm == algorithm);
    let value = decode(&matching.next().ok_or_else(invalid)?.digest)?;
    if matching.next().is_some() {
        return Err(invalid());
    }
    Ok(value)
}

fn replay(events: impl IntoIterator<Item = [u8; 32]>) -> [u8; 32] {
    events
        .into_iter()
        .fold([0; 32], |pcr, event| extend_pcr(&pcr, &event))
}

fn events(json: &[u8]) -> Result<(Vec<Event>, Vec<Event>), Error> {
    let records: Vec<Record> = serde_json::from_slice(json).map_err(|_| invalid())?;
    let mut previous = 0;
    let mut applications = Vec::new();
    let mut secure_boot = Vec::new();
    for record in records {
        if record.recnum <= previous {
            return Err(invalid());
        }
        previous = record.recnum;
        if record.pcr != 4 && record.pcr != 7 {
            continue;
        }
        if record.content_type.as_deref() != Some("pcclient_std") {
            return Err(invalid());
        }
        let content = record.content.ok_or_else(invalid)?;
        if content.event_type == "EV_NO_ACTION" {
            continue;
        }
        let event = Event {
            number: record.recnum,
            kind: content.event_type,
            data: decode(&content.event_data)?,
            digest: digest(&record.digests, "sha256")?
                .try_into()
                .map_err(|_| invalid())?,
        };
        if record.pcr == 4 {
            applications.push(event);
        } else {
            secure_boot.push(event);
        }
    }
    Ok((applications, secure_boot))
}

fn payload_valid(event: &Event) -> Result<(), Error> {
    if sha256(&event.data) != event.digest {
        return Err(invalid());
    }
    Ok(())
}

fn application_prefix(
    events: &[Event],
    shim: &[u8; 32],
    current_images: &[[u8; 32]],
) -> Result<usize, Error> {
    let mut separator = false;
    let mut action = false;
    for (index, event) in events.iter().enumerate() {
        match event.kind.as_str() {
            "EV_SEPARATOR" if !separator && event.data == [0, 0, 0, 0] => {
                payload_valid(event)?;
                separator = true;
            }
            "EV_EFI_ACTION"
                if !action
                    && event.data.strip_suffix(&[0]).unwrap_or(&event.data)
                        == b"Calling EFI Application from Boot Option" =>
            {
                payload_valid(event)?;
                action = true;
            }
            "EV_EFI_BOOT_SERVICES_APPLICATION" if separator && action => {
                if current_images.is_empty()
                    || events.len() != index + 1 + current_images.len()
                    || event.digest != *shim
                    || events[index + 1..]
                        .iter()
                        .zip(current_images)
                        .any(|(event, expected)| {
                            event.kind != "EV_EFI_BOOT_SERVICES_APPLICATION"
                                || event.digest != *expected
                        })
                {
                    return Err(invalid());
                }
                return Ok(index + 1);
            }
            _ => return Err(invalid()),
        }
    }
    Err(invalid())
}

struct ImageAuthority {
    certificate: Vec<u8>,
    mok: Option<Vec<u8>>,
    first_image: usize,
}

struct Captured {
    baseline: Baseline,
    shim_authority: Vec<u8>,
    image_authorities: Vec<ImageAuthority>,
}

fn authority_certificate(variable: &EfiVariable, db: &[u8]) -> Result<Vec<u8>, Error> {
    match (variable.vendor, variable.name.as_str()) {
        (SHIM_GUID, "Shim") => Ok(variable.data.clone()),
        (SHIM_GUID, "MokListRT") if variable.data.len() > 16 => Ok(variable.data[16..].to_vec()),
        (DB_GUID, "db") => signature_entries(db)
            .ok_or_else(invalid)?
            .into_iter()
            .find(|entry| entry.is_x509() && entry.encode() == variable.data)
            .map(|entry| entry.data)
            .ok_or_else(invalid),
        _ => Err(invalid()),
    }
}

fn capture_log(
    json: &[u8],
    live: &Measurements,
    shim: &[u8; 32],
    current_images: &[[u8; 32]],
) -> Result<Captured, Error> {
    let (applications, secure_boot) = events(json)?;
    if replay(applications.iter().map(|e| e.digest)) != live.applications
        || replay(secure_boot.iter().map(|e| e.digest)) != live.secure_boot
    {
        return Err(invalid());
    }
    let stage_index = application_prefix(&applications, shim, current_images)?;
    let shim_number = applications[stage_index - 1].number;
    let stage_number = applications[stage_index].number;
    let mut variable_names = Vec::new();
    let mut db = None;
    let mut separator = false;
    let mut authorities = Vec::new();
    let mut shim_policy_names = Vec::new();
    for event in &secure_boot {
        payload_valid(event)?;
        match event.kind.as_str() {
            "EV_EFI_VARIABLE_DRIVER_CONFIG" if !separator && authorities.is_empty() => {
                let variable = EfiVariable::decode(&event.data).ok_or_else(invalid)?;
                let expected = match variable.name.as_str() {
                    "SecureBoot" | "PK" | "KEK" => GLOBAL_GUID,
                    "db" | "dbx" | "dbt" | "dbr" => DB_GUID,
                    _ => return Err(invalid()),
                };
                if variable.vendor != expected
                    || (variable.name == "SecureBoot" && variable.data != [1])
                {
                    return Err(invalid());
                }
                if variable.name == "db" {
                    db = Some(variable.data);
                }
                variable_names.push(variable.name);
            }
            "EV_SEPARATOR"
                if !separator && authorities.is_empty() && event.data == [0, 0, 0, 0] =>
            {
                separator = true
            }
            "EV_EFI_VARIABLE_AUTHORITY" if separator => {
                let variable = EfiVariable::decode(&event.data).ok_or_else(invalid)?;
                if variable.vendor == SHIM_GUID
                    && matches!(variable.name.as_str(), "SbatLevel" | "MokSBState")
                {
                    if authorities.len() != 1
                        || event.number <= shim_number
                        || event.number >= stage_number
                        || shim_policy_names.contains(&variable.name)
                        || (variable.name == "MokSBState" && variable.data != [0])
                    {
                        return Err(invalid());
                    }
                    shim_policy_names.push(variable.name);
                } else {
                    authorities.push(event);
                }
            }
            _ => return Err(invalid()),
        }
    }
    let required = ["SecureBoot", "PK", "KEK", "db", "dbx"];
    let names: Vec<_> = variable_names.iter().map(String::as_str).collect();
    if !names.starts_with(&required)
        || !matches!(
            &names[required.len()..],
            [] | ["dbt"] | ["dbr"] | ["dbt", "dbr"]
        )
        || authorities.len() < 2
    {
        return Err(invalid());
    }
    let db = db.ok_or_else(invalid)?;
    let firmware = EfiVariable::decode(&authorities[0].data).ok_or_else(invalid)?;
    if firmware.vendor != DB_GUID || firmware.name != "db" || authorities[0].number >= shim_number {
        return Err(invalid());
    }
    let shim_authority = authority_certificate(&firmware, &db)?;
    if authorities[1].number <= shim_number || authorities[1].number >= stage_number {
        return Err(invalid());
    }
    let mut image_authorities = Vec::new();
    for (index, event) in authorities[1..].iter().enumerate() {
        if authorities[1..index + 1]
            .iter()
            .any(|previous| previous.data == event.data)
        {
            return Err(invalid());
        }
        // shim 15.8 verifies its second stage before PCR4; shim_lock.Verify measures first.
        let first_image = if index == 0 {
            0
        } else {
            applications[stage_index + 1..]
                .iter()
                .rposition(|image| image.number < event.number)
                .ok_or_else(invalid)?
                + 1
        };
        if image_authorities
            .iter()
            .any(|authority: &ImageAuthority| authority.first_image == first_image)
        {
            return Err(invalid());
        }
        let variable = EfiVariable::decode(&event.data).ok_or_else(invalid)?;
        image_authorities.push(ImageAuthority {
            certificate: authority_certificate(&variable, &db)?,
            mok: (variable.vendor == SHIM_GUID && variable.name == "MokListRT")
                .then_some(variable.data),
            first_image,
        });
    }
    let prefix_end = secure_boot
        .iter()
        .position(|e| e.number == authorities[1].number)
        .ok_or_else(invalid)?;
    Ok(Captured {
        baseline: Baseline {
            version: 1,
            applications_prefix: applications[..stage_index]
                .iter()
                .map(|e| e.digest)
                .collect(),
            secure_boot_prefix: secure_boot[..prefix_end].iter().map(|e| e.digest).collect(),
            firmware_db: sha256(&db),
        },
        shim_authority,
        image_authorities,
    })
}

async fn pe_digests(image: &Path) -> Result<Vec<Digest>, Error> {
    #[derive(Deserialize)]
    struct Lock {
        records: Vec<PeRecord>,
    }
    #[derive(Deserialize)]
    struct PeRecord {
        pcr: u32,
        digests: Vec<Digest>,
    }
    let output = Command::new(PCRLOCK)
        .args(["lock-pe", "--pcr=4", "--pcrlock=-", "--json=short"])
        .arg(image)
        .invoke(ErrorKind::Bios)
        .await?;
    let mut lock: Lock = serde_json::from_slice(&output).map_err(|_| invalid())?;
    if lock.records.len() != 1 || lock.records[0].pcr != 4 {
        return Err(invalid());
    }
    Ok(lock.records.remove(0).digests)
}

async fn matches_image(image: &Path, der: &[u8]) -> Result<bool, Error> {
    let cert = X509::from_der(der).map_err(|_| invalid())?;
    if cert.to_der()? != der {
        return Err(invalid());
    }
    let workspace = TmpDir::new().await?;
    let pem = workspace.join("authority.pem");
    write_file_atomic(&pem, cert.to_pem()?).await?;
    // A negative signature oracle is distinct from an invocation/parse failure.
    let output = Command::new("sbverify")
        .env("LC_ALL", "C")
        .arg("--cert")
        .arg(&pem)
        .arg(image)
        .kill_on_drop(true)
        .output()
        .await?;
    if output.status.success() {
        return Ok(true);
    }
    if output.status.code() == Some(1)
        && String::from_utf8_lossy(&output.stdout).trim() == "Signature verification failed"
    {
        return Ok(false);
    }
    Err(invalid())
}

async fn verify_image(image: &Path, der: &[u8]) -> Result<(), Error> {
    if !matches_image(image, der).await? {
        return Err(invalid());
    }
    Ok(())
}

async fn efivar(name: &str) -> Result<Vec<u8>, Error> {
    let bytes = tokio::fs::read(Path::new(EFIVARS).join(name)).await?;
    Ok(bytes.get(4..).ok_or_else(invalid)?.to_vec())
}

/// Captures a caller-authenticated current chain, retaining only firmware and shim.
/// Images follow shim in exact PCR4 order: for legacy StartOS, GRUB then the running
/// `/boot/vmlinuz-*`. Every measured application needs an immutable artifact; none
/// of this current tail is authorized by future predictions. Initial trust remains
/// the caller's obligation even when every artifact matches the public event log.
/// Certificate authority ordering follows shim 15.8's loader and shim_lock.Verify;
/// unknown firmware-tail layouts and hash-only authority events are unsupported.
pub async fn capture(known_shim: &Path, current_images: &[&Path]) -> Result<Baseline, Error> {
    let shim = digest(&pe_digests(known_shim).await?, "sha256")?
        .try_into()
        .map_err(|_| invalid())?;
    let mut images = Vec::with_capacity(current_images.len());
    for image in current_images {
        images.push(
            digest(&pe_digests(image).await?, "sha256")?
                .try_into()
                .map_err(|_| invalid())?,
        );
    }
    let json = Command::new(PCRLOCK)
        .args(["cel", "--json=short"])
        .invoke(ErrorKind::Bios)
        .await?;
    let live = tpm::read_measurements().await?;
    let captured = capture_log(
        &json,
        &Measurements {
            applications: live.applications,
            secure_boot: live.secure_boot,
        },
        &shim,
        &images,
    )?;
    let mok_list = if captured
        .image_authorities
        .iter()
        .any(|authority| authority.mok.is_some())
    {
        efivar(MOK_FILE).await?
    } else {
        Vec::new()
    };
    verify_current_chain(&captured, known_shim, current_images, &mok_list).await?;
    if sha256(&efivar(DB_FILE).await?) != captured.baseline.firmware_db {
        return Err(invalid());
    }
    Ok(captured.baseline)
}

async fn verify_current_chain(
    captured: &Captured,
    shim: &Path,
    current_images: &[&Path],
    mok_list: &[u8],
) -> Result<(), Error> {
    verify_image(shim, &captured.shim_authority).await?;
    let mok_entries = signature_entries(mok_list).ok_or_else(invalid)?;
    for authority in &captured.image_authorities {
        verify_image(
            current_images
                .get(authority.first_image)
                .ok_or_else(invalid)?,
            &authority.certificate,
        )
        .await?;
        if let Some(mok) = &authority.mok {
            if !mok_entries
                .iter()
                .any(|entry| entry.is_x509() && entry.first_in_list && entry.encode() == *mok)
            {
                return Err(invalid());
            }
        }
    }
    for (index, image) in current_images.iter().enumerate() {
        let mut verified = false;
        for authority in &captured.image_authorities {
            if authority.first_image <= index
                && matches_image(image, &authority.certificate).await?
            {
                verified = true;
                break;
            }
        }
        if !verified {
            return Err(invalid());
        }
    }
    Ok(())
}

async fn image_eligible(der: &[u8]) -> Result<(), Error> {
    let cert = X509::from_der(der).map_err(|_| invalid())?;
    if cert.to_der()? != der {
        return Err(invalid());
    }
    let output = Command::new("openssl")
        .args([
            "x509",
            "-inform",
            "DER",
            "-noout",
            "-ext",
            "extendedKeyUsage",
        ])
        .input(Some(&mut Cursor::new(der)))
        .invoke(ErrorKind::Bios)
        .await?;
    if String::from_utf8(output)?.contains("1.3.6.1.4.1.2312.16.1.2") {
        return Err(invalid());
    }
    Ok(())
}

fn predicted(
    baseline: &Baseline,
    image: &[u8; 32],
    mok: &EfiSignature,
) -> Result<Measurements, Error> {
    if baseline.version != 1
        || baseline.applications_prefix.is_empty()
        || baseline.secure_boot_prefix.is_empty()
        || !mok.is_x509()
    {
        return Err(invalid());
    }
    let authority = EfiVariable {
        vendor: SHIM_GUID,
        name: "MokListRT".into(),
        data: mok.encode(),
    }
    .encode();
    Ok(Measurements {
        applications: extend_pcr(&replay(baseline.applications_prefix.iter().copied()), image),
        secure_boot: extend_pcr(
            &replay(baseline.secure_boot_prefix.iter().copied()),
            &sha256(&authority),
        ),
    })
}

/// Predicts one finished UKI using an already-enrolled machine certificate (DER).
/// The caller authenticates the UKI as its single-machine-MOK-signed build artifact.
pub async fn predict(
    baseline: &Baseline,
    uki: &Path,
    machine_certificate: &[u8],
) -> Result<Measurements, Error> {
    predict_enrolled(
        baseline,
        uki,
        machine_certificate,
        &efivar(MOK_FILE).await?,
        &efivar(DB_FILE).await?,
    )
    .await
}

async fn predict_enrolled(
    baseline: &Baseline,
    uki: &Path,
    machine_certificate: &[u8],
    mok_list: &[u8],
    db: &[u8],
) -> Result<Measurements, Error> {
    image_eligible(machine_certificate).await?;
    let mok_entries = signature_entries(mok_list).ok_or_else(invalid)?;
    let mut matching = mok_entries
        .iter()
        .filter(|e| e.is_x509() && e.data == machine_certificate);
    let mok = matching.next().ok_or_else(invalid)?;
    // shim 15.8 check_db_cert_in_ram tries the first X.509 entry in each list.
    if !mok.first_in_list || matching.next().is_some() {
        return Err(invalid());
    }
    verify_image(uki, machine_certificate).await?;
    let hashes = pe_digests(uki).await?;
    let image: [u8; 32] = digest(&hashes, "sha256")?
        .try_into()
        .map_err(|_| invalid())?;
    if sha256(db) != baseline.firmware_db {
        return Err(invalid());
    }
    // Competing authorities can change which PCR7 event shim emits.
    for entry in signature_entries(db)
        .ok_or_else(invalid)?
        .iter()
        .chain(mok_entries.iter().filter(|e| *e != mok))
    {
        if entry.is_x509() {
            if matches_image(uki, &entry.data).await? {
                return Err(invalid());
            }
        } else if entry.is_sha256() {
            if entry.data == image {
                return Err(invalid());
            }
        } else if entry.is_sha1() {
            if entry.data == digest(&hashes, "sha1")? {
                return Err(invalid());
            }
        } else {
            return Err(invalid());
        }
    }
    predicted(baseline, &image, mok)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    const CERT: &str = "3082031d30820205a00302010202144630c5d190898b4f712e70c6e817b997cb4ae57c300d06092a864886f70d01010b0500301e311c301a06035504030c136c6f616465722d756e69742d66697874757265301e170d3236313131323030333633335a170d3236313131333030333633335a301e311c301a06035504030c136c6f616465722d756e69742d6669787475726530820122300d06092a864886f70d01010105000382010f003082010a028201010089c8c3355df06b7175f1ab5cff79c655558aa4a3b7bcf3714d52254561b882dbd850f609a99eba7b5fbaff7668c5559ac6768a39159ae94cd612762c5318f899d4ed51d4808c02e8371b493ee603e3b4b023969c63e42347d92ae4e552b6156cfc016a6e9a8506fd5f53b6ed3ba62a923c2451612e128dd8bd8feb5f12758d1b88769dafbf71fbb0c05b8ca11a582ece64b142e31cdae1d9a895b29625012a6fa67ba55ec7f7689d8fa85ca125f866f9c7471441f65479b956e268574a9108b9167403081b6d1ba42a5e753549809bd7c5c2041c26efcf3a71cd2a1db87de21e8c995b68d13ec3730dfb9a9a992f96edb6de703b9b40f36cf7c8280c712483610203010001a3533051301d0603551d0e04160414e94716a03da942ab6244e8e26da43787af72972b301f0603551d23041830168014e94716a03da942ab6244e8e26da43787af72972b300f0603551d130101ff040530030101ff300d06092a864886f70d01010b050003820101006dc2776d40b232667c3c2a6e51900a9ecabdf1e9d2191ed96a6c48b54bed230eb861fdc43fe3397df833b54d17c14ff90ec9142d71ead454a2b90daf7b9ed4d49615c565e784372f1b5e2996ab0ec1a73ea8ac0d5b0b127ddd3b0e7a025b1f3ce389409ba4bf1cb13962ffbb4c19554c5adbc2dfd27fc6d01ef767a3a916f7dc0606b5e1907535de2d28afbfd91919d3ed326f666753f9b8755d8a2eff9e4183d267b4fcc4e6b9139d5380e02c1465aa6a1468dada1aab77b17584341f9405da5401f2ff742c76e296d70ccab00821b76361c44aeb41be69f0707739ceae8794cd6c8a406913c618e970f683ed1b6798009ff2d79b3dc1129a563784bc7a6d19";
    fn cert() -> Vec<u8> {
        hex::decode(CERT).unwrap()
    }
    fn value(s: &str) -> [u8; 32] {
        hex::decode(s).unwrap().try_into().unwrap()
    }
    fn signature() -> EfiSignature {
        EfiSignature {
            kind: hex::decode("a159c0a5e494a74a87b5ab155c2bf072")
                .unwrap()
                .try_into()
                .unwrap(),
            owner: [0x44; 16],
            data: cert(),
            first_in_list: true,
        }
    }
    fn list(entry: &EfiSignature) -> Vec<u8> {
        let signature = entry.encode();
        [
            entry.kind.as_slice(),
            &((28 + signature.len()) as u32).to_le_bytes(),
            &0u32.to_le_bytes(),
            &(signature.len() as u32).to_le_bytes(),
            &signature,
        ]
        .concat()
    }
    fn event(pcr: u8, recnum: u64, kind: &str, data: Vec<u8>, digest: Option<[u8; 32]>) -> Value {
        json!({"pcr":pcr,"recnum":recnum,"digests":[{"hashAlg":"sha256","digest":hex::encode(digest.unwrap_or_else(|| sha256(&data)))}],"content_type":"pcclient_std","content":{"event_type":kind,"event_data":hex::encode(data)}})
    }
    fn variable(vendor: [u8; 16], name: &str, data: Vec<u8>) -> Vec<u8> {
        EfiVariable {
            vendor,
            name: name.into(),
            data,
        }
        .encode()
    }
    fn fixture() -> Vec<Value> {
        vec![
            event(
                4,
                1,
                "EV_EFI_ACTION",
                b"Calling EFI Application from Boot Option".to_vec(),
                None,
            ),
            event(4, 2, "EV_SEPARATOR", vec![0; 4], None),
            event(
                7,
                3,
                "EV_EFI_VARIABLE_DRIVER_CONFIG",
                variable(GLOBAL_GUID, "SecureBoot", vec![1]),
                None,
            ),
            event(
                7,
                4,
                "EV_EFI_VARIABLE_DRIVER_CONFIG",
                variable(GLOBAL_GUID, "PK", vec![]),
                None,
            ),
            event(
                7,
                5,
                "EV_EFI_VARIABLE_DRIVER_CONFIG",
                variable(GLOBAL_GUID, "KEK", vec![]),
                None,
            ),
            event(
                7,
                6,
                "EV_EFI_VARIABLE_DRIVER_CONFIG",
                variable(DB_GUID, "db", list(&signature())),
                None,
            ),
            event(
                7,
                7,
                "EV_EFI_VARIABLE_DRIVER_CONFIG",
                variable(DB_GUID, "dbx", vec![]),
                None,
            ),
            event(7, 8, "EV_SEPARATOR", vec![0; 4], None),
            event(
                7,
                9,
                "EV_EFI_VARIABLE_AUTHORITY",
                variable(DB_GUID, "db", signature().encode()),
                None,
            ),
            event(
                4,
                10,
                "EV_EFI_BOOT_SERVICES_APPLICATION",
                vec![],
                Some([0x11; 32]),
            ),
            event(
                7,
                11,
                "EV_EFI_VARIABLE_AUTHORITY",
                variable(SHIM_GUID, "SbatLevel", b"sbat,1,2024010900\n".to_vec()),
                None,
            ),
            event(
                7,
                12,
                "EV_EFI_VARIABLE_AUTHORITY",
                variable(SHIM_GUID, "Shim", cert()),
                None,
            ),
            event(
                4,
                13,
                "EV_EFI_BOOT_SERVICES_APPLICATION",
                vec![],
                Some([0x22; 32]),
            ),
        ]
    }
    fn live() -> Measurements {
        Measurements {
            applications: value("e2987a24b357ea99c8c5e7ce434f8a519d091e8c796cd2bd36c6dc8f864f7512"),
            secure_boot: value("8c6f0d2911de76bbf7dead6f6bad5a0e0490141abf0c96b0824f18a693be87dd"),
        }
    }
    fn capture_fixture(rows: &[Value], live: &Measurements) -> Result<Captured, Error> {
        capture_log(
            &serde_json::to_vec(rows).unwrap(),
            live,
            &[0x11; 32],
            &[[0x22; 32]],
        )
    }

    #[test]
    fn golden_replay_and_new_uki_replace_both_image_and_authority() {
        let captured = capture_fixture(&fixture(), &live()).unwrap();
        let baseline: Baseline =
            serde_json::from_slice(&serde_json::to_vec(&captured.baseline).unwrap()).unwrap();
        let predicted = predicted(&baseline, &[0x33; 32], &signature()).unwrap();
        assert_eq!(
            predicted.applications,
            value("f67746808ddfe1be305660ac5e50243e25580938c2bd90b1a8b407c7b0110627")
        );
        assert_eq!(
            predicted.secure_boot,
            value("64cbe4c6bdbc08bced287643ba0c385ec3b2bce23667aeac7587b02af02f01fb")
        );
        assert_ne!(predicted.secure_boot, live().secure_boot);
        assert_eq!(captured.shim_authority, cert());
        assert_eq!(captured.image_authorities[0].certificate, cert());
        let mut other_owner = signature();
        other_owner.owner[0] ^= 1;
        assert_ne!(
            predicted.secure_boot,
            super::predicted(&baseline, &[0x33; 32], &other_owner)
                .unwrap()
                .secure_boot
        );
    }

    #[test]
    fn public_log_must_replay_to_both_live_pcrs() {
        for pcr in [4, 7] {
            let mut measurements = live();
            if pcr == 4 {
                measurements.applications[0] ^= 1;
            } else {
                measurements.secure_boot[0] ^= 1;
            }
            assert!(capture_fixture(&fixture(), &measurements).is_err());
        }
    }

    #[test]
    fn applications_reject_detours_fallback_raw_hash_and_embedded_kernel() {
        let (events, _) = super::events(&serde_json::to_vec(&fixture()).unwrap()).unwrap();
        assert_eq!(
            application_prefix(&events, &[0x11; 32], &[[0x22; 32]]).unwrap(),
            3
        );
        for position in 0..=events.len() {
            let mut mutated = super::events(&serde_json::to_vec(&fixture()).unwrap())
                .unwrap()
                .0;
            mutated.insert(
                position,
                Event {
                    number: 99,
                    kind: "EV_EFI_BOOT_SERVICES_APPLICATION".into(),
                    data: vec![],
                    digest: [0x77; 32],
                },
            );
            assert!(application_prefix(&mutated, &[0x11; 32], &[[0x22; 32]]).is_err());
        }
        assert!(application_prefix(&events, &[0x22; 32], &[[0x11; 32]]).is_err());
        assert!(application_prefix(&events, &[0x11; 32], &[[0x99; 32]]).is_err());
        let mut rows = fixture();
        rows[0]["content"]["event_data"] = json!(hex::encode(
            b"Returning from EFI Application from Boot Option"
        ));
        let (events, _) = super::events(&serde_json::to_vec(&rows).unwrap()).unwrap();
        assert!(application_prefix(&events, &[0x11; 32], &[[0x22; 32]]).is_err());
    }

    #[test]
    fn cel_rejects_missing_duplicate_wrong_length_digest_and_record_order() {
        for replacement in [
            json!([]),
            json!([{"hashAlg":"sha1","digest":"00"}]),
            json!([{"hashAlg":"sha256","digest":"00"}]),
            json!([{"hashAlg":"sha256","digest":"00"},{"hashAlg":"sha256","digest":"00"}]),
        ] {
            let mut rows = fixture();
            rows[9]["digests"] = replacement;
            assert!(capture_fixture(&rows, &live()).is_err());
        }
        let mut rows = fixture();
        rows[10]["recnum"] = json!(9);
        assert!(capture_fixture(&rows, &live()).is_err());
    }

    #[test]
    fn authority_payloads_and_variable_lengths_are_not_merely_replayed() {
        let mut rows = fixture();
        rows[11]["content"]["event_data"] =
            json!(hex::encode(variable(SHIM_GUID, "unrecognized", cert())));
        assert!(capture_fixture(&rows, &live()).is_err());
        let mut rows = fixture();
        rows[11]["content"]["event_data"] = json!("00");
        assert!(capture_fixture(&rows, &live()).is_err());
        let mut rows = fixture();
        rows.swap(8, 11);
        rows[8]["recnum"] = json!(9);
        rows[11]["recnum"] = json!(12);
        let (a, s) = super::events(&serde_json::to_vec(&rows).unwrap()).unwrap();
        let changed = Measurements {
            applications: replay(a.iter().map(|e| e.digest)),
            secure_boot: replay(s.iter().map(|e| e.digest)),
        };
        assert!(capture_fixture(&rows, &changed).is_err());
    }

    async fn certificate(directory: &Path, name: &str, module_only: bool) -> Vec<u8> {
        let mut command = Command::new("openssl");
        command
            .args([
                "req",
                "-new",
                "-x509",
                "-newkey",
                "rsa:2048",
                "-nodes",
                "-subj",
                &format!("/CN={name}"),
                "-days",
                "1",
            ])
            .arg("-keyout")
            .arg(directory.join(format!("{name}.key")))
            .arg("-out")
            .arg(directory.join(format!("{name}.pem")));
        if module_only {
            command.args(["-addext", "extendedKeyUsage=1.3.6.1.4.1.2312.16.1.2"]);
        }
        command.invoke(ErrorKind::Bios).await.unwrap();
        X509::from_pem(
            &tokio::fs::read(directory.join(format!("{name}.pem")))
                .await
                .unwrap(),
        )
        .unwrap()
        .to_der()
        .unwrap()
    }

    #[tokio::test]
    async fn module_only_machine_certificates_are_ineligible() {
        let directory = TmpDir::new().await.unwrap();
        assert!(image_eligible(&cert()).await.is_ok());
        let module = certificate(&directory, "module", true).await;
        assert!(image_eligible(&module).await.is_err());
        assert!(image_eligible(b"not a DER certificate").await.is_err());
    }

    #[tokio::test]
    #[ignore = "requires systemd 257 pcrlock, systemd-boot-efi, sbsigntool and binutils; no EFI or TPM"]
    async fn software_pe_prediction_checks_real_signatures_enrollment_and_new_image() {
        let directory = TmpDir::new().await.unwrap();
        let machine = certificate(&directory, "machine", false).await;
        let firmware = certificate(&directory, "firmware", false).await;
        let mut mok = signature();
        mok.data = machine.clone();
        let mut firmware_entry = signature();
        firmware_entry.data = firmware;
        let db = list(&firmware_entry);
        let enrolled = list(&mok);
        let mut baseline = capture_fixture(&fixture(), &live()).unwrap().baseline;
        baseline.firmware_db = sha256(&db);
        let stub = Path::new("/usr/lib/systemd/boot/efi/linuxx64.efi.stub");
        let first = directory.join("first.efi");
        Command::new("sbsign")
            .arg("--key")
            .arg(directory.join("machine.key"))
            .arg("--cert")
            .arg(directory.join("machine.pem"))
            .arg("--output")
            .arg(&first)
            .arg(stub)
            .invoke(ErrorKind::Bios)
            .await
            .unwrap();
        let one = predict_enrolled(&baseline, &first, &machine, &enrolled, &db)
            .await
            .unwrap();
        assert!(verify_image(&first, &machine).await.is_ok());
        assert!(!matches_image(&first, &firmware_entry.data).await.unwrap());
        assert!(
            predict_enrolled(&baseline, &first, &machine, &[], &db)
                .await
                .is_err()
        );
        assert!(
            predict_enrolled(&baseline, &first, &machine, &enrolled, &enrolled)
                .await
                .is_err()
        );
        let mut competing = enrolled.clone();
        competing.extend_from_slice(&enrolled);
        assert!(
            predict_enrolled(&baseline, &first, &machine, &competing, &db)
                .await
                .is_err()
        );
        let mut wrong_owner = mok.clone();
        wrong_owner.owner[0] ^= 1;
        let two_owners = [enrolled.clone(), list(&wrong_owner)].concat();
        assert!(
            predict_enrolled(&baseline, &first, &machine, &two_owners, &db)
                .await
                .is_err()
        );
        let firmware_signed = directory.join("firmware.efi");
        Command::new("sbsign")
            .arg("--key")
            .arg(directory.join("firmware.key"))
            .arg("--cert")
            .arg(directory.join("firmware.pem"))
            .arg("--output")
            .arg(&firmware_signed)
            .arg(stub)
            .invoke(ErrorKind::Bios)
            .await
            .unwrap();
        assert!(
            predict_enrolled(&baseline, &firmware_signed, &machine, &enrolled, &db)
                .await
                .is_err()
        );
        let unsigned = directory.join("changed.efi");
        let payload = directory.join("initrd");
        tokio::fs::write(&payload, b"changed authenticated initrd fixture")
            .await
            .unwrap();
        Command::new("objcopy")
            .arg("--add-section")
            .arg(format!(".initrd={}", payload.display()))
            .arg(stub)
            .arg(&unsigned)
            .invoke(ErrorKind::Bios)
            .await
            .unwrap();
        let second = directory.join("second.efi");
        Command::new("sbsign")
            .arg("--key")
            .arg(directory.join("machine.key"))
            .arg("--cert")
            .arg(directory.join("machine.pem"))
            .arg("--output")
            .arg(&second)
            .arg(&unsigned)
            .invoke(ErrorKind::Bios)
            .await
            .unwrap();
        let two = predict_enrolled(&baseline, &second, &machine, &enrolled, &db)
            .await
            .unwrap();
        assert_ne!(one.applications, two.applications);
        assert_eq!(one.secure_boot, two.secure_boot);
        let mut changed_owner = mok;
        changed_owner.owner[0] ^= 1;
        let owner_prediction =
            predict_enrolled(&baseline, &second, &machine, &list(&changed_owner), &db)
                .await
                .unwrap();
        assert_eq!(two.applications, owner_prediction.applications);
        assert_ne!(two.secure_boot, owner_prediction.secure_boot);
        let mut shared_db = baseline.clone();
        shared_db.firmware_db = sha256(&enrolled);
        assert!(
            predict_enrolled(&shared_db, &second, &machine, &enrolled, &enrolled)
                .await
                .is_err()
        );
    }

    fn legacy_fixture() -> Vec<Value> {
        let mut rows = fixture();
        rows.push(event(
            4,
            14,
            "EV_EFI_BOOT_SERVICES_APPLICATION",
            vec![],
            Some([0x55; 32]),
        ));
        rows
    }

    fn measurements_for(rows: &[Value]) -> Measurements {
        let (applications, secure_boot) =
            super::events(&serde_json::to_vec(rows).unwrap()).unwrap();
        Measurements {
            applications: replay(applications.iter().map(|event| event.digest)),
            secure_boot: replay(secure_boot.iter().map(|event| event.digest)),
        }
    }

    fn capture_legacy(rows: &[Value], live: &Measurements) -> Result<Captured, Error> {
        capture_log(
            &serde_json::to_vec(rows).unwrap(),
            live,
            &[0x11; 32],
            &[[0x22; 32], [0x55; 32]],
        )
    }

    fn assert_future_only(baseline: &Baseline) {
        let future = predicted(baseline, &[0x33; 32], &signature()).unwrap();
        assert_eq!(
            future.applications,
            value("f67746808ddfe1be305660ac5e50243e25580938c2bd90b1a8b407c7b0110627")
        );
        assert_eq!(
            future.secure_boot,
            value("64cbe4c6bdbc08bced287643ba0c385ec3b2bce23667aeac7587b02af02f01fb")
        );
        assert_ne!(
            future.applications,
            value("3427500a8a1c083e02b647d1282bf8c2de00ffdcf72963b5d0c3137849da247d")
        );
        assert_ne!(
            future.applications,
            value("eda2b378f0dbce26e47fe5f42e20728a22f3c7ac3154e63560895841d2e87390")
        );
    }

    #[test]
    fn legacy_grub_kernel_capture_drops_tail_and_reuses_deduplicated_authority() {
        let measurements = Measurements {
            applications: value("3427500a8a1c083e02b647d1282bf8c2de00ffdcf72963b5d0c3137849da247d"),
            secure_boot: live().secure_boot,
        };
        let captured = capture_legacy(&legacy_fixture(), &measurements).unwrap();
        assert_eq!(captured.image_authorities.len(), 1);
        assert_eq!(captured.image_authorities[0].first_image, 0);
        assert_eq!(captured.baseline.applications_prefix.len(), 3);
        assert_eq!(
            captured.baseline.applications_prefix.last(),
            Some(&[0x11; 32])
        );
        assert_future_only(&captured.baseline);
        assert!(capture_fixture(&legacy_fixture(), &measurements).is_err());
        let mut wrong_live = measurements.clone();
        wrong_live.applications[0] ^= 1;
        assert!(capture_legacy(&legacy_fixture(), &wrong_live).is_err());
        wrong_live = measurements;
        wrong_live.secure_boot[0] ^= 1;
        assert!(capture_legacy(&legacy_fixture(), &wrong_live).is_err());
    }

    #[test]
    fn legacy_unknown_applications_and_wrong_tail_order_are_not_learned() {
        let mut rows = legacy_fixture();
        rows.push(event(
            4,
            15,
            "EV_EFI_BOOT_SERVICES_APPLICATION",
            vec![],
            Some([0x66; 32]),
        ));
        assert!(capture_legacy(&rows, &measurements_for(&rows)).is_err());
        let mut rows = legacy_fixture();
        rows[12]["digests"][0]["digest"] = json!(hex::encode([0x55; 32]));
        rows[13]["digests"][0]["digest"] = json!(hex::encode([0x22; 32]));
        assert!(capture_legacy(&rows, &measurements_for(&rows)).is_err());
        let rows = legacy_fixture();
        assert!(
            capture_log(
                &serde_json::to_vec(&rows).unwrap(),
                &measurements_for(&rows),
                &[0x11; 32],
                &[]
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn legacy_pcr7_suffix_is_associated_with_known_kernel_then_discarded() {
        let directory = TmpDir::new().await.unwrap();
        let mut kernel_mok = signature();
        kernel_mok.data = certificate(&directory, "legacy-kernel", false).await;
        let mut rows = legacy_fixture();
        rows.push(event(
            7,
            15,
            "EV_EFI_VARIABLE_AUTHORITY",
            variable(SHIM_GUID, "MokListRT", kernel_mok.encode()),
            None,
        ));
        let captured = capture_legacy(&rows, &measurements_for(&rows)).unwrap();
        assert_eq!(captured.image_authorities.len(), 2);
        assert_eq!(captured.image_authorities[1].first_image, 1);
        assert_eq!(captured.image_authorities[1].certificate, kernel_mok.data);
        assert_eq!(
            captured.image_authorities[1].mok.as_ref(),
            Some(&kernel_mok.encode())
        );
        assert_future_only(&captured.baseline);

        let mut before_kernel = rows.clone();
        before_kernel.swap(13, 14);
        before_kernel[13]["recnum"] = json!(14);
        before_kernel[14]["recnum"] = json!(15);
        assert!(capture_legacy(&before_kernel, &measurements_for(&before_kernel)).is_err());

        let mut unknown = rows.clone();
        unknown[14] = event(
            7,
            15,
            "EV_EFI_VARIABLE_AUTHORITY",
            variable(SHIM_GUID, "unknown", kernel_mok.encode()),
            None,
        );
        assert!(capture_legacy(&unknown, &measurements_for(&unknown)).is_err());

        let mut duplicate = rows.clone();
        duplicate.push(event(
            7,
            16,
            "EV_EFI_VARIABLE_AUTHORITY",
            variable(SHIM_GUID, "MokListRT", kernel_mok.encode()),
            None,
        ));
        assert!(capture_legacy(&duplicate, &measurements_for(&duplicate)).is_err());

        let mut late_policy = rows.clone();
        late_policy.push(event(
            7,
            16,
            "EV_EFI_VARIABLE_AUTHORITY",
            variable(SHIM_GUID, "SbatLevel", b"sbat,1,2024010900\n".to_vec()),
            None,
        ));
        assert!(capture_legacy(&late_policy, &measurements_for(&late_policy)).is_err());
    }

    async fn signed_fixture(directory: &Path, name: &str, key: &str) -> std::path::PathBuf {
        let payload = directory.join(format!("{name}.payload"));
        tokio::fs::write(&payload, name.as_bytes()).await.unwrap();
        let unsigned = directory.join(format!("{name}.unsigned"));
        Command::new("objcopy")
            .arg("--add-section")
            .arg(format!(".fixture={}", payload.display()))
            .arg("/usr/lib/systemd/boot/efi/linuxx64.efi.stub")
            .arg(&unsigned)
            .invoke(ErrorKind::Bios)
            .await
            .unwrap();
        let signed = directory.join(format!("{name}.efi"));
        Command::new("sbsign")
            .arg("--key")
            .arg(directory.join(format!("{key}.key")))
            .arg("--cert")
            .arg(directory.join(format!("{key}.pem")))
            .arg("--output")
            .arg(&signed)
            .arg(&unsigned)
            .invoke(ErrorKind::Bios)
            .await
            .unwrap();
        signed
    }

    #[tokio::test]
    #[ignore = "requires systemd 257 pcrlock, systemd-boot-efi, sbsigntool and binutils; no EFI or TPM"]
    async fn software_legacy_chain_verifies_known_signed_kernel_and_pcr7_suffix() {
        let directory = TmpDir::new().await.unwrap();
        let firmware = certificate(&directory, "firmware", false).await;
        let grub = certificate(&directory, "grub", false).await;
        let kernel = certificate(&directory, "kernel", false).await;
        let shim_path = signed_fixture(&directory, "shim", "firmware").await;
        let grub_path = signed_fixture(&directory, "grub", "grub").await;
        let kernel_path = signed_fixture(&directory, "kernel", "kernel").await;
        let mut hashes = Vec::<[u8; 32]>::new();
        for image in [&shim_path, &grub_path, &kernel_path] {
            hashes.push(
                digest(&pe_digests(image).await.unwrap(), "sha256")
                    .unwrap()
                    .try_into()
                    .unwrap(),
            );
        }
        assert_ne!(hashes[1], hashes[2]);
        let mut firmware_entry = signature();
        firmware_entry.data = firmware;
        let mut kernel_entry = signature();
        kernel_entry.data = kernel;
        let mok_list = list(&kernel_entry);
        let mut rows = legacy_fixture();
        rows[5] = event(
            7,
            6,
            "EV_EFI_VARIABLE_DRIVER_CONFIG",
            variable(DB_GUID, "db", list(&firmware_entry)),
            None,
        );
        rows[8] = event(
            7,
            9,
            "EV_EFI_VARIABLE_AUTHORITY",
            variable(DB_GUID, "db", firmware_entry.encode()),
            None,
        );
        rows[9] = event(
            4,
            10,
            "EV_EFI_BOOT_SERVICES_APPLICATION",
            vec![],
            Some(hashes[0]),
        );
        rows[11] = event(
            7,
            12,
            "EV_EFI_VARIABLE_AUTHORITY",
            variable(SHIM_GUID, "Shim", grub.clone()),
            None,
        );
        rows[12] = event(
            4,
            13,
            "EV_EFI_BOOT_SERVICES_APPLICATION",
            vec![],
            Some(hashes[1]),
        );
        rows[13] = event(
            4,
            14,
            "EV_EFI_BOOT_SERVICES_APPLICATION",
            vec![],
            Some(hashes[2]),
        );
        rows.push(event(
            7,
            15,
            "EV_EFI_VARIABLE_AUTHORITY",
            variable(SHIM_GUID, "MokListRT", kernel_entry.encode()),
            None,
        ));
        let captured = capture_log(
            &serde_json::to_vec(&rows).unwrap(),
            &measurements_for(&rows),
            &hashes[0],
            &hashes[1..],
        )
        .unwrap();
        verify_current_chain(
            &captured,
            &shim_path,
            &[&grub_path, &kernel_path],
            &mok_list,
        )
        .await
        .unwrap();
        assert!(
            verify_current_chain(&captured, &shim_path, &[&grub_path, &kernel_path], &[])
                .await
                .is_err()
        );
        assert!(
            verify_current_chain(
                &captured,
                &shim_path,
                &[&kernel_path, &grub_path],
                &mok_list
            )
            .await
            .is_err()
        );

        let mut db_kernel = rows.clone();
        let both_entries = [list(&firmware_entry), list(&kernel_entry)].concat();
        db_kernel[5] = event(
            7,
            6,
            "EV_EFI_VARIABLE_DRIVER_CONFIG",
            variable(DB_GUID, "db", both_entries),
            None,
        );
        db_kernel[14] = event(
            7,
            15,
            "EV_EFI_VARIABLE_AUTHORITY",
            variable(DB_GUID, "db", kernel_entry.encode()),
            None,
        );
        let from_db = capture_log(
            &serde_json::to_vec(&db_kernel).unwrap(),
            &measurements_for(&db_kernel),
            &hashes[0],
            &hashes[1..],
        )
        .unwrap();
        verify_current_chain(&from_db, &shim_path, &[&grub_path, &kernel_path], &[])
            .await
            .unwrap();
        assert!(from_db.image_authorities[1].mok.is_none());
        assert_eq!(from_db.image_authorities[1].first_image, 1);
        assert_eq!(
            from_db.baseline.secure_boot_prefix.len(),
            captured.baseline.secure_boot_prefix.len()
        );

        let shared_kernel = directory.join("kernel-shared.efi");
        Command::new("sbsign")
            .arg("--key")
            .arg(directory.join("grub.key"))
            .arg("--cert")
            .arg(directory.join("grub.pem"))
            .arg("--output")
            .arg(&shared_kernel)
            .arg(directory.join("kernel.unsigned"))
            .invoke(ErrorKind::Bios)
            .await
            .unwrap();
        rows.pop();
        let shared = capture_log(
            &serde_json::to_vec(&rows).unwrap(),
            &measurements_for(&rows),
            &hashes[0],
            &hashes[1..],
        )
        .unwrap();
        verify_current_chain(&shared, &shim_path, &[&grub_path, &shared_kernel], &[])
            .await
            .unwrap();
        assert!(
            verify_current_chain(&shared, &shim_path, &[&grub_path, &kernel_path], &[])
                .await
                .is_err()
        );
        assert_eq!(
            captured.baseline.secure_boot_prefix,
            shared.baseline.secure_boot_prefix
        );
        assert_eq!(
            captured.baseline.applications_prefix,
            shared.baseline.applications_prefix
        );
        assert_eq!(captured.baseline.applications_prefix.len(), 3);
        assert_eq!(
            captured.baseline.applications_prefix.last(),
            Some(&hashes[0])
        );
        let predicted = predicted(&captured.baseline, &[0x33; 32], &kernel_entry).unwrap();
        assert_ne!(predicted.applications, measurements_for(&rows).applications);
    }
}
