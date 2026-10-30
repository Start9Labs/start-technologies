# Start9 Accessory Onboarding

Status: draft for review. Owner: unassigned. Target: a StartOS minor release
after 0.4.1, unscheduled.

StartOS adopts Start9's own IoT products: with the Start9 app carrying the
onboarding over Bluetooth, it brings one onto the LAN, pairs with it, and grants
it to the services the user chooses. The Start9 Camera (a Seeed reCamera on
Start9 firmware) is the first; the design carries future Start9 products without
new OS or package code per product.

This document calls such a product an **accessory**. In start-core, "device"
already names the server's own hardware (`server_info.devices`, `server
device-info`, `hardwareRequirements.device`), a StartTunnel peer, the server
as the registry sees it (`registry/device_info.rs`), and an enrolled browser
signing key. A fifth meaning would collide with all four in the RPC surface,
the patch-db model and the UI.

## Background

### The pairing prototype

Pairing was prototyped in Home Assistant, on branches that have not merged or
shipped: `home-assistant-startos` `feat/camera-zeroconf-discovery` and
`reCamera-OS` `feat/start9-camera-mdns`. The `start9_camera` integration finds
a camera through HA's zeroconf, generates a 48-character token and three
passwords (system, Node-RED, stream), and sends them to the camera over plain
HTTP (`POST /start9-camera/pair` on port 8555). The camera accepts the first
token it sees within 60 minutes of boot and answers `409 already_paired` to any
other. HA keeps the token and passwords in its config entry and a journal in
`.storage`. StartOS reaches them only by calling the integration over HA's
websocket.

Wi-Fi provisioning is Improv over BLE (`reCamera-OS`,
`usr/lib/start9-camera/improv.py`): the HA companion app sends the SSID and
password in the clear over GATT, and the camera's Improv result is a
`my.home-assistant.io` redirect that opens the integration's setup.

Nothing from the prototype is deployed, so this design replaces it rather than
migrating from it. Its problems:

- **One consumer.** The camera belongs to one HA instance. Frigate, or any
  other service, cannot use it without HA releasing it first, and each would
  need its own pairing, journal and release logic.
- **Lifecycle tied to HA.** Uninstalling HA without deleting each camera leaves
  cameras locked with passwords that exist only in HA's backup; recovery is a
  factory reset by hand.
- **No authentication in either direction.** Any LAN host can claim an unpaired
  camera during its boot window, and the camera cannot tell which server it
  paired with. Wi-Fi credentials cross BLE in plaintext.
- **Discovery does not reach the consumer.** mDNS does not cross `lxcbr0`, so a
  package cannot see the camera's advertisement.

### What StartOS has today

- **Identity.** `AccountInfo` (`shared-libs/crates/start-core/src/account.rs`)
  holds a NIST P-256 root CA key and its self-signed certificate, generated at
  setup, stored in `private.key_store.local_certs`, and carried in the OS
  backup. `CertStore` (`net/ssl.rs`) holds an intermediate under it that signs
  every leaf certificate StartOS issues. Clients already trust this root to
  reach the UI over HTTPS. The only key derivation in the crate is
  `derive_tsig_key` (`net/dns_update/mod.rs`), HKDF-SHA256 over a tunnel
  peer's WireGuard PSK with a fixed label.
- **TLS.** Every acceptor is built with `with_no_client_auth()`, and no client
  configuration carries a certificate. `CertStore::cert_for` generates the
  leaf's key itself and keys leaves by hostname set; `SANInfo` holds DNS names
  and IP addresses. Nothing issues for a foreign key or verifies a client.
- **Private and public state.** `Database.private` (`db/model/private.rs`)
  holds secrets (`key_store`, SSH and developer keys, the password hash) and is
  never sent to the UI. `Database.public` has three members, `server_info`,
  `package_data` and `ui`, and is pushed live to every UI session
  (`db/mod.rs`, `subscribe`). Per-package host policy lives on the package's
  entry: `PackageDataEntry.outbound_gateway`, a value the user edits.
- **At rest.** The data drive is LUKS with the fixed key `DEFAULT_PASSWORD`
  (`disk/main.rs:21`), so `private` is effectively plaintext to anyone holding
  the disk.
- **mDNS.** The host runs avahi, which publishes the server's `.local` name.
  startd's only contact with it is shelling out to `avahi-resolve-host-name`
  (`net/mdns.rs`). zbus is a dependency, used for NetworkManager proxies in
  `net/gateway.rs`; there is no avahi proxy.
- **Effects.** A package reads host state through an effect whose handler
  watches a patch-db pointer and registers the package's callback in
  `DbWatchedCallbacks` (`service/callbacks.rs`); the SDK wraps it in a
  `Watchable` with `once()`, `watch()` and `const()`.
- **RPC clients.** A client enrolls an Ed25519 key with `auth.login` (the
  StartOS password, once) and signs each request with it, as `start-cli` does.
  An enrolled key has full admin rights, and one idle for 30 days is unenrolled.
- **Backup.** `OsBackup` (`backup/os.rs`) is version 2: `server_id`,
  `hostname`, `root_ca_key`, `root_ca_cert`, `ssh_key`, `compat_s9pk_key`,
  `ui`. Restore feeds `account` to `Database::init` and writes nothing else
  into `public`.
- **No accessory concepts.** No registry, no Bluetooth (postinst disables
  `bluetooth` and `hciuart`), no device passthrough beyond the manifest's
  fixed GPU/FUSE/TUN/KVM flags.
- **Gateway protocol.** `rfcs/draft-start9-agcp.md` (PR #4087) fixes the house
  conventions for a Start9 wire protocol: JSON-RPC 2.0 over HTTPS at a
  versioned path, TLS 1.3 with mutual authentication by self-signed
  certificates whose contents are ignored, identity as the SPKI fingerprint,
  one display form for fingerprints, and numbered error kinds. This design
  follows them.

## Goals

- A user adds a Start9 accessory from the Start9 app by scanning its QR code.
  The accessory joins the LAN and pairs with the server in one flow.
- The server needs no radio and never receives Wi-Fi credentials. They go from
  the user's phone to the accessory, over Bluetooth LE.
- Pairing is mutually authenticated: the accessory proves it holds the key on
  its QR code, and the server proves it is the server the QR code was scanned
  into.
- A paired accessory is granted to services by the user, any number at once,
  and revoked the same way. Packages read connection details for granted
  accessories through the SDK and speak the accessory's own protocols (RTSP,
  HTTP), not this one.
- StartOS persists only what it needs to authenticate accessories and to
  answer services. Anything the accessory can hold, it holds.
- Pairings survive an OS backup and restore.

## Non-goals

- Third-party devices. Home Assistant and other packages keep their own
  integrations for those.
- Matter, Thread, Zigbee and Z-Wave.
- Bluetooth on the server, and device passthrough.
- Remote (off-LAN) access to accessories.
- Firmware updates for accessories. The Start9 app manages them, without
  StartOS.
- Wi-Fi management. StartOS changes no paired accessory's network.
- Encrypted media (RTSPS). See Open questions.

## Alternatives considered

### Keep pairing in Home Assistant

The prototype. Every other consumer needs its own implementation, accessories
are locked to one consumer, and the lifecycle problems above remain.

### Matter

Matter's commissioning has the properties this needs: a QR setup code, PAKE,
attestation and operational certificates issued by the controller, and
multi-admin sharing. It is rejected for this scope because:

- A Wi-Fi device that is not yet on the LAN is commissioned over BLE or Wi-Fi
  PAF. Either needs a radio on the commissioner, and the server has none.
  Commissioning a device already on the LAN works, but then Wi-Fi provisioning
  is a separate problem anyway.
- It brings a full device stack (data model, interaction model, fabrics) to
  every accessory, of which this design would use only onboarding and
  sessions.
- Media is WebRTC; Frigate and most NVRs need RTSP, so cameras would carry a
  second protocol regardless.

Nothing here prevents an accessory from also speaking Matter to share sensor
entities with HA through multi-admin later.

### Wi-Fi Easy Connect (DPP)

A standard for exactly the Wi-Fi provisioning step, and a fit for Start9
because the camera, the server and StartWRT all run hostap. It needs either a
Wi-Fi radio on the server or a DPP relay in the access point, and most routers
have none. It is kept as a future onboarding route, and the QR code is a DPP
URI so that route needs no second sticker (see Accessory identity).

### Bluetooth on the server

Requires hardware most servers lack, device passthrough StartOS does not have,
and range: the server is often far from the accessory, while the user's phone
is next to it.

### Improv

Sends the Wi-Fi password in the clear, cannot authenticate either side, and has
no field for the server's identity. This design uses a Start9 GATT service
instead.

### Adding accessories from the StartOS UI

Carrying the bundle through the web UI needs Web Bluetooth, which Safari and iOS
home-screen apps lack. The native app reaches both platforms. A second route,
such as a QR code shown to the accessory's camera or delivery over the LAN to a
wired accessory, adds firmware and UI that the Wi-Fi camera does not use.

### Sealing the bundle in StartOS

Spares the iOS and Android apps HPKE, but passes the Wi-Fi password and the setup
secret through startd, which has no other use for either.

### A PAKE instead of HPKE

SPAKE2+ (as Matter) lets a short typed code stand in for a key. The QR code
already carries the accessory's full public key, so the onboarding bundle can
be sealed to it directly with HPKE (RFC 9180), which has library
implementations on both sides. A PAKE becomes necessary only for manual entry
of a short code (see Open questions).

### A certificate authority for accessories

StartOS could issue each accessory a certificate from its intermediate and
present a client certificate issued the same way. Rejected: no client
authentication, CSR signing or non-DNS SAN exists in `net/ssl.rs`, and the
registry is the revocation list either way, so a certificate adds issuance
code without adding trust. Pinning the SubjectPublicKeyInfo both ways, as
AGCP does, needs none of it.

### A dedicated server identity key for accessories

Rejected in favour of the root CA key (see Server identity). A separate key
would be one more secret to store and back up, and every client already
trusts the root.

### Deriving logins from the root CA key

Rejected in favour of a dedicated secret (see Credentials). No signing key in
start-core is used as key-derivation input, and the backup version has to
change for the registry anyway, so the dedicated secret costs nothing the
reuse would save.

### StartOS stores the logins it issues

Rejected in favour of deriving them. Storing them would put every login in
`private` and in the backup for no capability that derivation lacks.

## Design

### Overview

```
  Start9 app (phone)              StartOS                        accessory
                                  (startd)
  ───────────────────────────────────────────────────────────────────────
  scan QR, send public key ──────► begin session
  session, token, server key ◄────┘
  ask for Wi-Fi password
  seal bundle to factory key
  write bundle over BLE ────────────────────────────────────► open, join Wi-Fi
                                  browse mDNS ◄────────────── advertise
                                  mutual TLS, pin both ways ─► paired
                                  registry entry
```

Five parts:

1. **Accessory identity.** A factory key pair and a setup secret, both on the
   accessory's QR code.
2. **Onboarding.** The app seals a bundle to the accessory and carries it over
   BLE.
3. **Pairing.** On the LAN, the two sides authenticate each other and pin each
   other's keys.
4. **Credentials.** Logins on the accessory, derived by StartOS on demand.
5. **Grants.** The user grants accessories to services; the SDK delivers
   connection details.

### Accessory identity and QR code

Each accessory has, from manufacture:

- a P-256 **factory key pair**, which never leaves the accessory;
- a 128-bit **setup secret**.

Both survive a firmware flash: they live outside any partition that a flash or
factory reset erases. On the reCamera that excludes `/userdata`, which a flash
wipes.

The QR code is a DPP bootstrapping URI (Wi-Fi Easy Connect 2.0):

```
DPP:V:2;K:<base64 DER SubjectPublicKeyInfo of the factory key>;I:S9/camera/recamera/<base32 setup secret>;;
```

`K:` is the factory public key, which is also the accessory's DPP
bootstrapping key, so a future DPP route reads the same sticker. `I:` is the
DPP information field, carrying the Start9 marker, the accessory class, the
model and the setup secret. Accessory keys are P-256 because DPP bootstrapping
keys are NIST-curve keys, and P-256 is the one every DPP implementation
supports.

Encodings: `K:` is standard base64 with padding, as DPP has it, of the same
SubjectPublicKeyInfo bytes the accessory presents in TLS (an uncompressed
point); StartOS hashes the decoded bytes as given and never re-encodes the
key. The setup secret is RFC 4648 base32, uppercase and unpadded (26
characters). Parsers accept the fields in any order and ignore unknown ones
(`M:`, `C:`, `H:`). The reCamera's class is `camera` and its model
`recamera`.

Possession of the QR code is the authority to onboard the accessory while it
is unpaired. Once paired, the accessory ignores its setup secret until it is
released or factory reset.

An accessory's **identity** is its factory key. Its **fingerprint** is the
SHA-256 of the DER SubjectPublicKeyInfo, encoded and displayed exactly as AGCP
§Fingerprints specifies, so a fingerprint on the accessory's own display, on
the StartOS accessory page and on a router page compare character for
character. Every protocol field named `fingerprint` or `server` below carries
the AGCP wire form (unpadded base64url of the digest).

Its **accessory ID** is the lowercase, unpadded base32 of the first 10 octets
of the fingerprint digest (16 characters). It is derived from the factory
key, so StartOS computes it before it has spoken to the accessory.

### Server identity

StartOS's identity toward accessories is its root CA key. In TLS it presents
the root certificate it already holds, self-signed by that key; the accessory
ignores the certificate's contents and pins the key's fingerprint. The
justification is the one that carried the root-derived UPnP UDN in #3634: no
new state, already in the backup, already trusted by every client, and
regenerated by a factory reset.

The root key signs TLS CertificateVerify messages exactly as it signs
certificates. Its compromise already compromises every certificate StartOS
issues; this adds impersonating the server to accessories, and no new key to
lose.

### Onboarding

The app scans the QR code and calls, with its enrolled key:

```
accessory.onboard.begin { key, class, model }
  → { session, token, server, serverId, expires, id, wireFingerprint }
```

`key` is the QR code's `K:` value; the setup secret stays on the phone. startd
opens an onboarding session in memory (one-hour expiry, so the user can scan the
sticker and then unbox, mount and power the accessory). The app trusts `server`
because it arrived over TLS from a server whose root the phone already trusts.

The app then asks the user for the Wi-Fi network and password, which it sends
nowhere but the accessory, and seals a **bundle** with HPKE (RFC 9180):

- Mode `mode_psk`, KEM `DHKEM(P-256, HKDF-SHA256)`, KDF `HKDF-SHA256`, AEAD
  `AES-128-GCM`.
- Recipient key: the factory public key from the QR. PSK:
  `HKDF-SHA256(ikm = setup secret, salt = "", info = "start9-onboard-v1 psk", L = 32)`,
  `psk_id = "start9-onboard-v1"`. RFC 9180 requires a PSK of at least 32
  bytes; the 128-bit setup secret is stretched to that length, and its
  entropy is unchanged.
- `info = "start9 onboard v1"`, `aad` = the UTF-8 bytes of the session ID.
- `session` and `token` are each 16 random bytes, base64url.
- Plaintext, JSON:
  `{ "v": 1, "session", "token", "server": <server fingerprint>, "serverId",
"wifi": { "ssid", "psk", "security" }, "expires": <Unix seconds> }`, with
  `security` one of `wpa2-psk`, `wpa3-sae` or `open` (`psk` empty).

The bundle's text form, wherever it travels, is
`<session>.<base64url(enc || ciphertext)>`: the accessory needs the session ID
as AAD before it can open the bundle, so it travels in clear in front. The
session ID is not secret, and the AEAD rejects a bundle whose prefix was
changed; the accessory also checks it against the `session` inside. The text
form is about 500 characters, and up to about 730 with a 32-byte SSID and a
63-character passphrase.

`mode_psk` is the point: a sender must hold both the accessory's public key
and the setup secret. The public key is visible to anyone the accessory talks
to; the setup secret is only on the QR code. So only someone who scanned the
QR code can produce a bundle the accessory opens.

### Carrying the bundle

The app carries the bundle over native Bluetooth LE. The accessory advertises
the **Start9 onboarding GATT service**
`faa69064-50ae-4ce3-bbad-6e284048ad5b` for as long as it is unpaired, whether or
not it already has a network, so a released accessory can be added again:

| Characteristic | UUID                                   | Properties   | Value                                       |
| -------------- | -------------------------------------- | ------------ | ------------------------------------------- |
| `info`         | `9edb1604-db34-4de1-9c62-c445b44b5893` | read         | JSON `{ v, id, class, model, fingerprint }` |
| `bundle`       | `9592ec0b-f1d6-4c16-be7c-a444069a6999` | write (long) | the bundle's text form, UTF-8, in chunks    |
| `status`       | `55cb7a10-cdc5-44c7-8376-061abbc0192d` | read, notify | JSON `{ state, error?, ipv4?, ipv6? }`      |

A GATT attribute value is at most 512 bytes, and a bundle with Wi-Fi can be
longer, so the app writes it in chunks of at most 512 bytes, each at offset 0.
The first chunk starts with the session and its dot; the later chunks contain
no dot and continue the bundle. A write that starts with a session and a dot
starts a new bundle. The framing needs no integrity of its own: the AEAD
rejects any bundle that was spliced or cut.

`status.state` is one of `idle`, `received`, `joining`, `connected` and
`error`, and `error` is `BUNDLE_REJECTED`, `ALREADY_PAIRED` or `WIFI_FAILED`.

The app scans for the service UUID, checks that `info.fingerprint` matches the
scanned key, writes `bundle`, and follows `status` until the accessory reports
an address or an error. It then polls `accessory.onboard.status` until pairing
finishes.

**Future routes** (not in this RFC): DPP from a server Wi-Fi radio or through a
StartWRT relay; Thread joining.

### Accessory behaviour on a bundle

1. Open the bundle. Failure (wrong key, wrong setup secret, tampering) is
   `BUNDLE_REJECTED` on `status`, and nothing else changes.
2. Reject it if `expires` has passed or the accessory is already paired.
3. Store `{ session, token, server }` as a pending pairing. A later valid
   bundle replaces it.
4. Add the network and join. Report the address on `status`, or the failure.
5. Advertise on mDNS (see Discovery) with `state=pending`.

An accessory that is not paired within the bundle's expiry discards the
pending pairing. Wi-Fi it joined stays configured. An accessory without a
real-time clock that has not yet set its time cannot check `expires`; it
accepts the bundle and discards the pending pairing an hour after receipt.

### Discovery

Accessories advertise `_start9._tcp` with a subtype per class
(`_camera._sub._start9._tcp`) on their contract port (8443 by default; the
SRV record is authoritative), with TXT:

- `v=1`, the contract version;
- `id`, the accessory ID;
- `state`, one of `unpaired`, `pending`, `paired`.

startd browses `_start9._tcp` through avahi, the host's mDNS responder, over
D-Bus with the zbus proxies it already uses for NetworkManager. The host is on
the LAN, so packages need no LAN access of their own. Browsing feeds:

- onboarding sessions waiting for their accessory;
- the address and online state of each paired accessory, written to its
  registry entry (see Persisted state) so grants and the UI react to it.

An advertisement is unauthenticated and its `id` is public, so a sighting is
only a candidate address. startd writes a paired accessory's `address` and
`online` only after `accessory.info` over pinned TLS succeeds at that address. A
candidate that fails is dropped and leaves the recorded address in place.

### Pairing

When startd sees its pending accessory (`state=pending`, matching `id`):

1. startd opens TLS to the accessory. The accessory presents its factory key
   in a self-signed certificate; startd checks the key against the session.
   startd presents its root certificate; the accessory checks the key against
   the `server` fingerprint in its pending pairing.
2. startd calls `pair.adopt { session, token }`. The accessory checks both
   against its pending pairing.
3. The accessory stores the server fingerprint durably, marks itself paired,
   stops onboarding (BLE service, its own access point), and disables its
   setup secret.
4. startd writes the registry entry with the accessory's fingerprint and
   pushes the owner logins (see Credentials).

From then on both sides authenticate by TLS alone, each against the key it
pinned: the accessory accepts only the pinned server as a client, and startd
accepts only the fingerprint in the registry as the server. No certificate
field is examined. Deleting the registry entry is revocation: the accessory
keeps a key startd will never again accept.

Pairing is authorised by the bundle, not by trust on first use: the accessory
accepts the server named in the bundle and no other, and the server accepts
the key on the QR code and no other, so neither side records a pending
identity for the user to approve.

### Credentials

The logins an accessory needs (on the reCamera: the system account, Node-RED,
the RTSP stream) are **stored on the accessory and derived by StartOS**, never
stored by StartOS:

```
password = base64url( HKDF-SHA256(
    ikm  = accessory_secret,
    salt = "start9-accessory-credentials-v1",
    info = accessory_id || 0x00 || purpose || 0x00 || generation,
    L    = 32 ) )[..32]
```

`generation` is its decimal ASCII form.

`accessory_secret` is 32 random bytes in `private.key_store`, generated the
first time a login is derived and carried in the OS backup from then on. It is
the one secret this design adds.

- `purpose` is `owner/<kind>` for logins shown to the user (system, Node-RED),
  `grant/<package-id>/<kind>` for logins given to one service, or
  `shared/<kind>` for logins every grantee receives.
- `generation` is a counter in the registry entry. Rotation increments it and
  pushes the new value.

startd pushes each login over the paired TLS connection
(`credential.set { kind, username, password }`). The accessory applies it with
its own mechanism and keeps what that mechanism keeps: a shadow hash, a bcrypt
hash, the RTSP server's plaintext. Because StartOS can recompute any login at
any time, it can hand logins to services, and show the user theirs, without
storing them, and while the accessory is offline.

Accessories declare each login kind in `accessory.info` `credentialKinds`,
with a **scope**: `owner` (shown to the user), `shared` (one login every
grantee receives) or `per-grant` (one login per service), and optionally the
fixed username their mechanism requires (the reCamera's system account is
`recamera`). Without one, startd uses the kind's name for owner and shared
logins and the package ID for per-grant ones. Scopes and usernames are what
startd needs to derive and label logins while the accessory is offline, so
startd keeps them on the registry entry (see Persisted state). The reCamera's
stream has one RTSP user, so its `rtsp` kind is **shared**: every grantee
gets the same login. Revoking a grant then rotates the shared login and
redelivers it to the remaining grantees. A push to an offline accessory is
retried when discovery next sees it; until it lands, the registry's generation
is ahead of the accessory's and the entry says so.

### Grants

A grant gives a service access to one capability of one accessory.

**Capabilities** are named by the accessory contract, for example
`start9.video.rtsp` and `start9.detections.http`. An accessory reports its own
in `accessory.info`, and startd records them on the registry entry.

**The grant is user state on the grantee**, following `outbound_gateway`:

```
package_data.<package-id>.accessories: { <accessory-id>: [<capability>, ...] }
```

An empty list, or a missing accessory, is revocation. The field defaults to
empty, so existing data needs no migration. There is no manifest field: any
package can be granted any capability, and a package ignores a capability it
does not use. The RPC is
`package.set-accessories { id, accessories }`, replacing the whole map; the UI
reads it from patch-db.

**Granting** is a user action, never a package action, and a package cannot
request one. The app, the accessory page's **Used by** list and the service page's **Accessories** section
all edit the same map.

**Effect.** `get-accessories { capability?, callback? }` returns, for the
calling package, only accessories granted to it:

```ts
{
  id, name, class, model, online,
  capabilities: {
    'start9.video.rtsp': { url: 'rtsp://192.168.1.40:8554/live', username, password },
  },
}
```

startd builds each `url` from the capability's scheme, port and path in
`accessory.info` and the accessory's IPv4 address, else its bracketed IPv6
address. An offline accessory is still listed, with `online: false` and the
URL from its last known address, or `""` when startd knows none since boot.

The handler watches two patch-db pointers through one callback: the package's
own `accessories` map, and `server_info.accessories`, where address, online
state, reported capabilities and login generations live. Any change to either
fires it, so the SDK's `.const()` pattern reruns the caller:

```ts
const cameras = await sdk.accessories.getAll(effects, { capability: 'start9.video.rtsp' }).const()
```

**Packages** turn that into their application's configuration. Frigate's
package writes its `cameras:` section through a file model.

**Actions** remain the place for application-specific configuration of a
granted accessory: Frigate's zones and retention per camera. They take a `dynamicSelect` over `sdk.accessories.getAll`, as
`pairedCameraSelect` does on the prototype branch (`startos/cameras.ts`).

**Uninstall** drops the package's entry, which rotates any shared logins.

### Accessory settings

Settings that belong to the accessory, not to a service (name, detection
classes, owner logins, release) live on the StartOS Accessories page. Whether
accessories may describe further settings for StartOS to render is an open
question.

### Removal

**Remove** on the accessory page:

1. startd calls `pair.release` over the paired connection. The accessory
   erases its server pin and StartOS-issued logins, restores its unpaired
   defaults, re-enables its setup secret and resumes onboarding. It keeps its
   Wi-Fi.
2. startd deletes the registry entry and every package's grants for it.

Release is best effort. If the accessory is offline, startd records
`releasing` in the entry and retries whenever discovery sees the accessory.
The user can instead forget it immediately; the accessory then stays locked
to a server that will never accept it, until it is factory reset, and the UI
says so.

### Persisted state

The registry is a map on `server_info`, beside the server's other
network-facing state:

```
server_info.accessories: {
  <accessory-id>: {
    name, class, model,
    fingerprint,
    state: 'paired' | 'releasing',
    credentials: { <purpose>: generation },
    appliedCredentials: { <purpose>: generation },
    credentialKinds: { <kind>: { scope, username? } },
    capabilities: [{ id, scheme, port, path, credentialKind? }, ...],
    address: { ipv4?, ipv6? } | null,
    online: bool,
  }
}
```

Two kinds of field share the entry. `name`, `class`, `model`, `fingerprint`,
`state`, `credentials`, `appliedCredentials` and `credentialKinds` are
**persisted intent**: written by the user or by pairing, carried in the
backup. `appliedCredentials` holds the generation the accessory last
confirmed for each purpose, so an entry whose generation is ahead says a push
is still due. `credentialKinds` is copied from `accessory.info` at pairing and
refreshed on each later `accessory.info`. `capabilities`, `address` and `online` are
**observed**: written by discovery and by `accessory.info`, reset to empty,
null and false on boot and on restore, never carried in the backup. They live
in patch-db because the UI and the effect react to them; nothing else holds
them.

In `private.key_store`: `accessory_secret`, created on first use.

The field defaults to empty, so an existing server needs no migration.

Not persisted:

- **Logins.** Derived on demand.
- **Onboarding sessions.** In memory.

**Backup.** `OsBackup` becomes version 3, adding `accessory_secret` and the
persisted fields of `server_info.accessories`. Restore writes them into
`private.key_store` and `server_info`, the first thing restore writes into
`public`. A version 2 backup restores with neither, as a server that paired
nothing. A restored server has the same root key, so it is still the server
its accessories pin, and the same `accessory_secret`, so it re-derives every
login.

### Accessory contract v1

The accessory serves JSON-RPC 2.0 over HTTPS at `POST /start9/v1`, on port
8443 by default (the mDNS SRV record is authoritative), under the
AGCP transport rules: TLS 1.3 only, the server requests a client certificate,
certificate contents are ignored and identity is the SubjectPublicKeyInfo,
every request carries an `id`, parameters are named, member names are
lowerCamelCase and enumerated values lowercase with hyphens, and errors carry
`data.kind`. The accessory's key is P-256; StartOS accepts P-256 and Ed25519.

| Method               | Callable by                            | Purpose                                                                                              |
| -------------------- | -------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `accessory.info`     | anyone                                 | `{ v, id, class, model, name, swVersion, state, fingerprint, capabilities, credentialKinds }`, below |
| `pair.adopt`         | the server named in the pending bundle | `{ session, token }`                                                                                 |
| `credential.set`     | the pinned server                      | `{ kind, username, password }`                                                                       |
| `pair.release`       | the pinned server                      | no parameters                                                                                        |
| capability endpoints | per capability                         | e.g. the camera's `/start9-camera/state`                                                             |

| Code | Kind           | Meaning                                                        |
| ---- | -------------- | -------------------------------------------------------------- |
| 1001 | NOT_AUTHORIZED | The client's key is not the pinned server.                     |
| 1002 | ALREADY_PAIRED | The accessory is paired and the method needs it unpaired.      |
| 1003 | NOT_PENDING    | No pending pairing, or the session or token does not match it. |
| 1005 | UNSUPPORTED    | The credential kind is not one the accessory offers.           |

Malformed parameters are JSON-RPC's own -32602, and a mechanism that fails
while applying a request is -32603; neither carries `data.kind`.

`accessory.info` for the reCamera:

```json
{
  "v": 1,
  "id": "<accessory ID>",
  "class": "camera",
  "model": "recamera",
  "name": "reCamera",
  "swVersion": "…",
  "state": "paired",
  "fingerprint": "<wire fingerprint>",
  "capabilities": [
    { "id": "start9.video.rtsp", "scheme": "rtsp", "port": 8554, "path": "/live", "credentialKind": "rtsp" },
    { "id": "start9.detections.http", "scheme": "http", "port": 8555, "path": "/start9-camera/state" }
  ],
  "credentialKinds": {
    "system": { "scope": "owner", "username": "recamera" },
    "node-red": { "scope": "owner", "username": "admin" },
    "rtsp": { "scope": "shared" }
  }
}
```

Each capability names the scheme, port and path startd needs to build the URL
it hands to services, and the login kind that goes with it, if any. Neither
the capabilities nor the usernames are secret; an unpaired accessory exposed
the same ports to the LAN already.

`credential.set` answers once the login is applied, which on the reCamera
includes a Node-RED restart of up to about a minute, so startd allows each
call 150 seconds and retries one that times out.

Repeating a request with the same values is harmless, so startd retries freely.

### UI

The Start9 app adds accessories. It signs in as any RPC client does, and the
phone must already trust the server's root CA. It needs its key only to add an
accessory, so one unenrolled after 30 idle days is replaced by signing in again.

**Finding the server.** StartOS advertises `_startos._tcp` on port 443 with a
static avahi service file, named for the server's hostname, beside its `.local`
name. The app browses for it and lists what it finds, usually one server, so the
user picks instead of typing an address; typing remains for a server it cannot
see. An advertisement is unauthenticated, and that costs nothing here: the app
sends the password only over TLS to a root the phone trusts.

- **Add accessory (app):** scan the QR code → enter the Wi-Fi network and
  password → write the bundle over BLE → progress → name → grant to services.
- **Accessories page (StartOS):** paired accessories with status, name,
  fingerprint, **Used by**, owner logins and **Remove**.
- **Service page (StartOS):** an Accessories section listing grants and offering
  paired accessories.

## Security considerations

- **The QR code is the authority while an accessory is unpaired.** Anyone who
  reads it can onboard the accessory to their server. The setup secret is
  disabled once paired, so a QR code read afterwards is useless until the
  accessory is released or reset.
- **Confidentiality.** Wi-Fi credentials go from the phone to the accessory
  inside the HPKE bundle and never reach StartOS. Logins cross the LAN inside
  mutual TLS. Neither depends on BLE link security.
- **Replay.** A bundle is bound to a session, a token and an expiry; the
  accessory takes at most one pending pairing and refuses bundles once paired.
- **Spoofing on the LAN.** Each side accepts only the key it pinned, so a
  diverted control connection fails the handshake. Unlike AGCP, no step trusts a
  first identity: both pins come from the QR code and the bundle. A forged mDNS
  advertisement cannot move a paired accessory's address (see Discovery), so it
  cannot point a grantee's stream login at another host. A host that diverts the
  address itself, by ARP or Neighbor Discovery spoofing, still receives the
  grantee's plain RTSP connection and can ask for the login in the clear.
- **Root key in TLS.** The root key already signs every certificate StartOS
  issues; signing CertificateVerify is the same operation. A compromised root
  key could already impersonate the server to every client, and can now
  impersonate it to accessories. It cannot derive logins.
- **Accessory secret.** One new 32-byte secret in `private`, under the same
  LUKS default key as everything else there. Its compromise yields every
  accessory login; rotating it means rotating every login, which the
  generation counters support.
- **At rest on the accessory.** The accessory holds its factory key, setup
  secret, server pin and logins; the reCamera has no secure element, so
  someone with its flash has that accessory's identity and logins and nothing
  else.
- **Revocation** is the registry. A removed accessory's key is refused
  because its fingerprint is no longer listed.
- **The app's key** has full admin rights over the server, since StartOS has no
  scoped keys.
- **Wi-Fi password exposure.** On a WPA2/WPA3-Personal network the accessory
  receives the shared password, as any client does.

## Limitations and tradeoffs

- **Same network.** Discovery is mDNS, so the server and the accessory must share
  a broadcast domain. An accessory on another VLAN or an isolated Wi-Fi network
  never pairs, and its session expires. User documentation covers this.
- **Changed Wi-Fi.** Nothing pushes a new password to a paired accessory. A
  paired reCamera off the LAN for five minutes turns its access point back on,
  where its own web UI can change the network with the owner login; failing that,
  the user factory resets it, forgets it in StartOS and adds it again. User
  documentation covers this.
- **Shared logins.** Accessories without per-grant logins (the reCamera's
  stream) give every grantee the same login; revoking one grantee rotates it
  for the rest.
- **Plain RTSP.** Media is unencrypted on the LAN, as in the prototype.
- **Start9 accessories only.** Third-party devices are out of scope.
- **Every package is offered every accessory.** With no manifest declaration
  the grant UI cannot know which services can use a camera; it lists them all.

## Product decisions

These are UX calls, listed for @MattDHill's concept ack rather than decided
here:

- The page's name and placement ("Accessories" under the server's settings,
  or a top-level entry).
- Whether finishing onboarding in the app offers grants to installed services,
  and how the list is ordered with no manifest signal.
- The wording of **Forget** versus waiting for an offline accessory to be
  released, and of the "locked to this server" consequence.

## Assumptions to validate

- The iOS and Android Bluetooth APIs can scan by a 128-bit service UUID and write
  a bundle of up to about 730 bytes, in chunks of up to 512 (long writes), to the
  reCamera's BlueZ 5.58 GATT server.
- Both apps can seal HPKE `mode_psk` with this suite, through a platform or
  third-party library, or on their own HKDF, P-256 and AES-GCM.
- A static avahi service file publishes on the same interfaces as the server's
  `.local` name, including when mDNS is turned off for a gateway.
- HPKE `mode_psk` with P-256 can be implemented on the camera's Node 22.8
  `crypto` (ECDH, HKDF, AES-GCM), or a small pure-JS HPKE library runs
  acceptably there without a JIT.
- The Node TLS server on the camera can require a client certificate, accept
  a self-signed CA-flagged root certificate as one, and check its SPKI.
- rustls on startd can present the root certificate as a client certificate
  and verify a self-signed server certificate by SPKI alone.
- startd can browse through avahi's D-Bus API on every gateway without
  affecting its own `.local` publication.
- Somewhere on the reCamera's eMMC can hold the factory key and setup secret
  across a flash and a factory reset.

## Testing

- Unit tests for the bundle (seal/open, wrong PSK, wrong key, tampered AAD,
  expiry) against RFC 9180 test vectors, in both apps and in the firmware.
- Unit tests for login derivation (purpose and generation separation, a fixed
  vector per kind) in Rust and the firmware simulator.
- An accessory simulator (Node, using the firmware's code) on a network
  namespace, for discovery, pairing, pinned-key rejection in both directions,
  credentials, release and offline retries.
- A forged advertisement carrying a paired accessory's ID leaves its registry
  address and grantee URLs unchanged.
- The app's add flow against the simulator.
- A version 2 backup restored onto this code, and a version 3 backup restored
  onto a fresh install with a paired simulator.
- On hardware: the app's Bluetooth flow on iOS and Android with a reCamera, a
  backup and restore with a paired camera, and a Frigate grant end to end.

## Rollout

1. Accessory contract v1, factory identity and the GATT service in
   `reCamera-OS`, replacing the prototype pairing on
   `feat/start9-camera-mdns` before it merges.
2. Registry, onboarding, pairing, credentials and discovery in startd; the
   `_startos._tcp` advertisement; the Accessories page; backup version 3.
3. `get-accessories`, the package grant map and the SDK; grants in the UI.
4. The Start9 app's add flow.
5. A Frigate package that consumes grants.

## Action items

1. **A persistent location for the factory identity.** Find or create a
   place on the reCamera that keeps the factory key and setup secret across
   a flash, a factory reset and an OTA upgrade. The firmware currently keeps
   them in a hidden directory on the eMMC's `RESERVED` partition
   (`/dev/mmcblk0p5`), Seeed's OTA download area: no flash or factory reset
   writes it, and Seeed's `upgrade.sh` empties it with a glob that skips
   hidden directories, so the identity survives only by that accident. A
   future upgrade script that wipes it differently, or a reflash of the
   whole eMMC or its partition table, makes the accessory generate a new
   identity, and its QR sticker stops matching. Start9 controls the
   firmware's partition layout and upgrade path, so the candidate is a small
   dedicated `FACTORY` partition, carved from `RESERVED`, written at Start9's
   flashing station and touched by no flash, reset or upgrade path. The
   eMMC's hardware boot partitions are unsuitable, since bootloader updates
   write there. The reCamera has no secure element.
2. **WPA3-only networks.** The reCamera joins WPA2 and WPA2/WPA3 transition
   networks. It cannot yet join a network that allows only SAE: its
   wpa_supplicant 2.9 is built without SAE, and the CYW43012 firmware it
   ships is Infineon's external-SAE build, which needs the host to run SAE
   through external authentication. The mainline brcmfmac in its 5.10 kernel
   offers only firmware-offloaded SAE. Supporting SAE-only networks needs
   Infineon's brcmfmac (or a backport of its external-authentication
   support), a wpa_supplicant with SAE and external authentication enabled,
   and a test on hardware. Enabling SAE in wpa_supplicant alone is not
   enough, and would make the firmware pick SAE where it cannot complete it.
   Until then a `wpa3-sae` network is joined as WPA-PSK with optional PMF,
   which works on transition networks, and the accessory reports
   `WIFI_FAILED` on an SAE-only one.

## Open questions

1. **Factory identity for reCameras Start9 flashes itself.** Who generates the
   key and setup secret, and how the user gets the QR code (a sticker from
   Start9's flashing station, or a page on the accessory's own access point).
   Where they are stored is action item 1.
2. **Manual entry.** The URI is about 150 characters. A short typed code needs
   a PAKE (as Matter's SPAKE2+) to be safe; v1 accepts only a scanned URI.
3. **Accessory-declared settings.** May an accessory describe its settings in
   `InputSpec`, so StartOS renders them with the action form engine?
4. **RTSPS.** Containers would need to trust the StartOS root CA, and every
   consumer would need to accept RTSPS.
