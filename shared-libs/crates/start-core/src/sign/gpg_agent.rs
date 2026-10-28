use std::sync::Arc;

use ed25519_dalek::{Signature, VerifyingKey};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::process::Command;

use crate::prelude::*;
use crate::util::Invoke;

/// Assuan caps a line at 1000 bytes; leaves room for `D `, escapes, and the newline.
const MAX_DATA_CHUNK: usize = 300;

/// An Ed25519 key in the GnuPG keyring, signing through gpg-agent.
#[derive(Clone, Debug)]
pub struct GpgAgentKey {
    spec: String,
    resolved: Arc<tokio::sync::OnceCell<(String, VerifyingKey)>>,
}
impl GpgAgentKey {
    /// Takes any key specifier `gpg` accepts. A trailing `!` selects that exact key or subkey.
    pub fn new(spec: &str) -> Self {
        Self {
            spec: spec.to_owned(),
            resolved: Default::default(),
        }
    }

    /// The keygrip and public key of the signing key the specifier selects.
    async fn resolve(&self) -> Result<&(String, VerifyingKey), Error> {
        self.resolved
            .get_or_try_init(|| async {
                let (lookup, exact) = match self.spec.strip_suffix('!') {
                    Some(id) => (id, true),
                    None => (&*self.spec, false),
                };
                let listing = Command::new("gpg")
                    .arg("--batch")
                    .arg("--with-colons")
                    .arg("--with-keygrip")
                    .arg("--list-secret-keys")
                    .arg("--")
                    .arg(lookup)
                    .invoke(ErrorKind::Authorization)
                    .await?;
                let keygrip =
                    select_signing_key(&String::from_utf8_lossy(&listing), exact.then_some(lookup))
                        .map_err(|candidates| {
                            let msg = if candidates.is_empty() {
                                t!("sign.gpg-agent.no-signing-key", key = &self.spec)
                            } else {
                                t!(
                                    "sign.gpg-agent.ambiguous-key",
                                    key = &self.spec,
                                    candidates = candidates.join(", ")
                                )
                            };
                            Error::new(eyre!("{msg}"), ErrorKind::InvalidRequest)
                        })?;
                let mut conn = AssuanConn::connect().await?;
                let res = conn.transact(&format!("READKEY {keygrip}"), None).await?;
                let key = parse_public_key(&res).ok_or_else(|| {
                    Error::new(
                        eyre!("{}", t!("sign.gpg-agent.malformed-response")),
                        ErrorKind::Deserialization,
                    )
                })?;
                Ok((keygrip, key))
            })
            .await
    }

    pub async fn verifying_key(&self) -> Result<VerifyingKey, Error> {
        Ok(self.resolve().await?.1)
    }

    /// Pure Ed25519 over the message.
    pub async fn sign(&self, msg: &[u8]) -> Result<Signature, Error> {
        let (keygrip, verifying_key) = self.resolve().await?;
        let mut conn = AssuanConn::connect().await?;
        conn.set_pinentry_options().await;
        conn.transact(&format!("SIGKEY {keygrip}"), None).await?;
        conn.transact("SETHASH --inquire", Some(msg)).await?;
        let res = conn.transact("PKSIGN", None).await?;
        let signature = parse_signature(&res).ok_or_else(|| {
            Error::new(
                eyre!("{}", t!("sign.gpg-agent.malformed-response")),
                ErrorKind::Deserialization,
            )
        })?;
        verifying_key.verify_strict(msg, &signature)?;
        Ok(signature)
    }
}

/// Picks the keygrip of the one usable Ed25519 signing key in a `gpg --with-colons` listing.
/// `exact` restricts it to the key whose fingerprint ends with that ID.
/// Errs with the fingerprints of every usable match unless exactly one exists.
fn select_signing_key(listing: &str, exact: Option<&str>) -> Result<String, Vec<String>> {
    #[derive(Default)]
    struct Key {
        usable: bool,
        fpr: Option<String>,
        grip: Option<String>,
    }
    let mut keys: Vec<Key> = Vec::new();
    for line in listing.lines() {
        let f: Vec<&str> = line.split(':').collect();
        let field = |i: usize| f.get(i).copied().unwrap_or_default();
        match field(0) {
            "sec" | "ssb" => keys.push(Key {
                usable: field(3) == "22"
                    && field(16).eq_ignore_ascii_case("ed25519")
                    && field(11).contains('s')
                    && !matches!(field(1), "r" | "e" | "d" | "i" | "n")
                    && !field(14).starts_with('#'),
                ..Key::default()
            }),
            "fpr" => {
                if let Some(k) = keys.last_mut().filter(|k| k.fpr.is_none()) {
                    k.fpr = Some(field(9).to_owned());
                }
            }
            "grp" => {
                if let Some(k) = keys.last_mut().filter(|k| k.grip.is_none()) {
                    k.grip = Some(field(9).to_owned());
                }
            }
            _ => (),
        }
    }
    let exact = exact.map(|id| id.trim_start_matches("0x").to_ascii_uppercase());
    let mut matches: Vec<(String, String)> = keys
        .into_iter()
        .filter(|k| k.usable)
        .filter_map(|k| Some((k.fpr?, k.grip?)))
        .filter(|(fpr, _)| exact.as_deref().map_or(true, |id| fpr.ends_with(id)))
        .collect();
    if matches.len() == 1 {
        Ok(matches.remove(0).1)
    } else {
        Err(matches.into_iter().map(|(fpr, _)| fpr).collect())
    }
}

struct AssuanConn {
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
}
impl AssuanConn {
    async fn connect() -> Result<Self, Error> {
        Command::new("gpgconf")
            .arg("--launch")
            .arg("gpg-agent")
            .invoke(ErrorKind::Authorization)
            .await?;
        let socket = String::from_utf8(
            Command::new("gpgconf")
                .arg("--list-dirs")
                .arg("agent-socket")
                .invoke(ErrorKind::Authorization)
                .await?,
        )?;
        let (read, writer) = UnixStream::connect(socket.trim()).await?.into_split();
        let mut conn = Self {
            reader: BufReader::new(read),
            writer,
        };
        conn.response(None).await?;
        Ok(conn)
    }

    /// Points pinentry at this terminal or display. The agent's own defaults stand otherwise.
    async fn set_pinentry_options(&mut self) {
        let tty = std::env::var("GPG_TTY").ok().or_else(|| {
            std::fs::read_link("/proc/self/fd/0")
                .ok()
                .map(|p| p.display().to_string())
                .filter(|p| p.starts_with("/dev/"))
        });
        let options = [
            ("ttyname", tty),
            ("ttytype", std::env::var("TERM").ok()),
            ("display", std::env::var("DISPLAY").ok()),
        ];
        for (name, value) in options {
            if let Some(value) = value {
                self.transact(&format!("OPTION {name}={value}"), None)
                    .await
                    .log_err();
            }
        }
    }

    /// Sends a command and collects its data lines. `inquiry` answers any `INQUIRE`.
    async fn transact(&mut self, command: &str, inquiry: Option<&[u8]>) -> Result<Vec<u8>, Error> {
        self.writer.write_all(command.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;
        self.response(inquiry).await.map_err(|e| {
            Error::new(
                eyre!(
                    "{}",
                    t!(
                        "sign.gpg-agent.command-failed",
                        command = command.split(' ').next().unwrap_or(command),
                        error = e.source
                    )
                ),
                ErrorKind::Authorization,
            )
        })
    }

    async fn response(&mut self, inquiry: Option<&[u8]>) -> Result<Vec<u8>, Error> {
        let mut data = Vec::new();
        let mut line = Vec::new();
        loop {
            line.clear();
            if self.reader.read_until(b'\n', &mut line).await? == 0 {
                return Err(Error::new(
                    eyre!("{}", t!("sign.gpg-agent.malformed-response")),
                    ErrorKind::Authorization,
                ));
            }
            let line = line.strip_suffix(b"\n").unwrap_or(&line);
            if line == b"OK" || line.starts_with(b"OK ") {
                return Ok(data);
            } else if let Some(err) = line.strip_prefix(b"ERR ") {
                return Err(Error::new(
                    eyre!("{}", String::from_utf8_lossy(err)),
                    ErrorKind::Authorization,
                ));
            } else if let Some(d) = line.strip_prefix(b"D ") {
                percent_decode(d, &mut data);
            } else if line.starts_with(b"INQUIRE ") {
                for chunk in inquiry.unwrap_or_default().chunks(MAX_DATA_CHUNK) {
                    self.writer.write_all(b"D ").await?;
                    self.writer.write_all(&percent_encode(chunk)).await?;
                    self.writer.write_all(b"\n").await?;
                }
                self.writer.write_all(b"END\n").await?;
            }
        }
    }
}

fn percent_encode(data: &[u8]) -> Vec<u8> {
    let mut res = Vec::with_capacity(data.len() * 3);
    for &b in data {
        if matches!(b, b'%' | b'\r' | b'\n') {
            res.extend_from_slice(format!("%{b:02X}").as_bytes());
        } else {
            res.push(b);
        }
    }
    res
}

fn percent_decode(bytes: &[u8], out: &mut Vec<u8>) {
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if let Some(b) = bytes
                .get(i + 1..i + 3)
                .and_then(|h| std::str::from_utf8(h).ok())
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
}

/// A canonical S-expression.
#[derive(Debug, PartialEq)]
enum Sexp {
    Atom(Vec<u8>),
    List(Vec<Sexp>),
}
impl Sexp {
    fn parse(bytes: &[u8]) -> Option<Self> {
        let (sexp, rest) = Self::parse_one(bytes)?;
        rest.is_empty().then_some(sexp)
    }
    fn parse_one(bytes: &[u8]) -> Option<(Self, &[u8])> {
        if *bytes.first()? == b'(' {
            let mut items = Vec::new();
            let mut rest = &bytes[1..];
            loop {
                if *rest.first()? == b')' {
                    return Some((Self::List(items), &rest[1..]));
                }
                let (item, r) = Self::parse_one(rest)?;
                items.push(item);
                rest = r;
            }
        }
        let colon = bytes.iter().position(|b| *b == b':')?;
        let len: usize = std::str::from_utf8(&bytes[..colon]).ok()?.parse().ok()?;
        let atom = bytes.get(colon + 1..colon + 1 + len)?;
        Some((Self::Atom(atom.to_vec()), &bytes[colon + 1 + len..]))
    }
    /// The atom following the first `(name value)` pair anywhere in the tree.
    fn value(&self, name: &[u8]) -> Option<&[u8]> {
        let Self::List(items) = self else {
            return None;
        };
        match items.as_slice() {
            [Self::Atom(n), Self::Atom(v)] if n == name => Some(v),
            _ => items.iter().find_map(|i| i.value(name)),
        }
    }
}

fn parse_public_key(bytes: &[u8]) -> Option<VerifyingKey> {
    let sexp = Sexp::parse(bytes)?;
    let curve = sexp.value(b"curve")?;
    if !matches!(
        curve,
        b"Ed25519" | b"ed25519" | b"1.3.6.1.4.1.11591.15.1" | b"1.3.101.112"
    ) {
        return None;
    }
    let q = sexp.value(b"q")?;
    let q = match q {
        [0x40, rest @ ..] if rest.len() == 32 => rest,
        q => q,
    };
    VerifyingKey::from_bytes(q.try_into().ok()?).ok()
}

fn parse_signature(bytes: &[u8]) -> Option<Signature> {
    let sexp = Sexp::parse(bytes)?;
    let mut sig = [0u8; 64];
    for (half, name) in [(0, b"r"), (32, b"s")] {
        let v = sexp.value(name)?;
        if v.len() > 32 {
            return None;
        }
        sig[half + 32 - v.len()..half + 32].copy_from_slice(v);
    }
    Some(Signature::from_bytes(&sig))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atom(s: &[u8]) -> Vec<u8> {
        [format!("{}:", s.len()).as_bytes(), s].concat()
    }

    #[test]
    fn parses_agent_public_key() {
        let key = ed25519_dalek::SigningKey::from_bytes(&[3; 32]).verifying_key();
        let q = [&[0x40][..], key.as_bytes()].concat();
        let sexp = [
            b"(".as_slice(),
            &atom(b"public-key"),
            b"(",
            &atom(b"ecc"),
            b"(",
            &atom(b"curve"),
            &atom(b"Ed25519"),
            b")(",
            &atom(b"flags"),
            &atom(b"eddsa"),
            b")(",
            &atom(b"q"),
            &atom(&q),
            b")))",
        ]
        .concat();
        assert_eq!(parse_public_key(&sexp), Some(key));
    }

    #[test]
    fn rejects_non_ed25519_key() {
        let sexp = [
            b"(".as_slice(),
            &atom(b"public-key"),
            b"(",
            &atom(b"ecc"),
            b"(",
            &atom(b"curve"),
            &atom(b"Curve25519"),
            b")(",
            &atom(b"q"),
            &atom(&[0x40; 33]),
            b")))",
        ]
        .concat();
        assert_eq!(parse_public_key(&sexp), None);
    }

    #[test]
    fn pads_short_signature_halves() {
        let r = [[0u8].as_slice(), &[9; 31]].concat();
        let sexp = [
            b"(".as_slice(),
            &atom(b"sig-val"),
            b"(",
            &atom(b"eddsa"),
            b"(",
            &atom(b"r"),
            &atom(&r[1..]),
            b")(",
            &atom(b"s"),
            &atom(&[7; 32]),
            b")))",
        ]
        .concat();
        let sig = parse_signature(&sexp).unwrap().to_bytes();
        assert_eq!(&sig[..32], r.as_slice());
        assert_eq!(&sig[32..], &[7; 32]);
    }

    /// A primary Ed25519 signing key with a cv25519 encryption subkey and a revoked Ed25519
    /// signing subkey, then an RSA key and an Ed25519 key whose secret is absent.
    const LISTING: &str = "\
sec:u:255:22:AAAA111111111111:1700000000:::u:::scSC:::+::ed25519:::0:
fpr:::::::::0000000000000000000000000000AAAA111111111111:
grp:::::::::1111111111111111111111111111111111111111:
uid:u::::1700000000::HASH::Test <test@example.invalid>::::::::::0:
ssb:u:255:18:BBBB222222222222:1700000000::::::e:::+::cv25519::
fpr:::::::::0000000000000000000000000000BBBB222222222222:
grp:::::::::2222222222222222222222222222222222222222:
ssb:r:255:22:CCCC333333333333:1700000000::::::s:::+::ed25519::
fpr:::::::::0000000000000000000000000000CCCC333333333333:
grp:::::::::3333333333333333333333333333333333333333:
sec:u:2048:1:DDDD444444444444:1700000000:::u:::scSC:::+::::::0:
fpr:::::::::0000000000000000000000000000DDDD444444444444:
grp:::::::::4444444444444444444444444444444444444444:
sec:u:255:22:EEEE555555555555:1700000000:::u:::scSC:::#::ed25519:::0:
fpr:::::::::0000000000000000000000000000EEEE555555555555:
grp:::::::::5555555555555555555555555555555555555555:
";

    #[test]
    fn selects_the_usable_ed25519_signing_key() {
        assert_eq!(
            select_signing_key(LISTING, None),
            Ok("1111111111111111111111111111111111111111".into())
        );
    }

    #[test]
    fn exact_selection_requires_a_usable_key() {
        assert_eq!(
            select_signing_key(LISTING, Some("0xaaaa111111111111")),
            Ok("1111111111111111111111111111111111111111".into())
        );
        assert_eq!(
            select_signing_key(LISTING, Some("CCCC333333333333")),
            Err(vec![])
        );
        assert_eq!(
            select_signing_key(LISTING, Some("BBBB222222222222")),
            Err(vec![])
        );
    }

    #[test]
    fn several_usable_keys_are_ambiguous() {
        let listing = LISTING.replace("ssb:r:255:22:CCCC", "ssb:u:255:22:CCCC");
        assert_eq!(
            select_signing_key(&listing, None),
            Err(vec![
                "0000000000000000000000000000AAAA111111111111".into(),
                "0000000000000000000000000000CCCC333333333333".into(),
            ])
        );
    }

    #[tokio::test]
    async fn agent_signs_pure_ed25519() {
        let Ok(spec) = std::env::var("START_CORE_TEST_GPG_KEY") else {
            return;
        };
        let signer = crate::sign::Signer::GpgAgent(GpgAgentKey::new(&spec));
        let key = signer.verifying_key().await.unwrap();
        let msg = b"line\nbreak % and \r and a long tail ".repeat(40);
        let sig = signer.sign_message(&msg).await.unwrap();
        let crate::sign::AnySignature::Ed25519(raw) = sig else {
            panic!("gpg-agent signs pure Ed25519");
        };
        let crate::sign::AnyVerifyingKey::Ed25519(vk) = &key;
        vk.verify_strict(&msg, &raw).unwrap();

        let commitment = crate::sign::commitment::blake3::Blake3Commitment {
            hash: crate::util::serde::Base64([1; 32]),
            size: 7,
        };
        let sig = signer
            .sign_commitment(&commitment, "startos")
            .await
            .unwrap();
        key.verify_commitment(&commitment, "startos", &sig).unwrap();
    }

    #[test]
    fn percent_coding_round_trips() {
        let data = b"a%b\r\nc\0".to_vec();
        let mut out = Vec::new();
        percent_decode(&percent_encode(&data), &mut out);
        assert_eq!(out, data);
    }
}
