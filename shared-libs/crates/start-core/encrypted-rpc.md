# Encrypted RPC integration

`util/net/secure.rs::EncryptedSocket` composes the [signed-session protocol](signed-session.md)
with the existing WebSocket transport. `sign/session.rs` owns identity proofs, ephemeral
DH, key derivation and record authentication. The adapter owns framing, not another actor,
heartbeat or reconnect task.

## Identity and trust

`sign/instance.rs` loads registry, Tunnel and WRT `rpc-identity.pem` files. Atomic
no-replace publication makes concurrent starts use the same completed winner. Invalid
files fail startup without rotation. Files use mode `0600`; file and directory contents
are synced. WRT preserves the file during sysupgrade. StartOS uses its existing account
developer identity. This identity is separate from enrollment, authcookies, package and
OS signing trust, and TLS keys.

`sign/fingerprint.rs` owns identity fingerprints: SHA-256 of raw Ed25519 public bytes,
formatted as `SHA256:` followed by lowercase hexadecimal. Extending key schemes also
requires extending that owner; transport code does not impose a signing scheme.

## Admission and authorization

`middleware/auth/session.rs::RpcSessionUpgrader` selects admission with
`secure=1&context=<selected-server-name>`. The existing `request_identities` policy checks
the selected name against ingress and loopback identities. Missing opt-in retains legacy
behavior; invalid versions, contexts and encrypted admission fail closed.

The client signs a fresh hello for its logical target, ordinary RPC or a particular
existing `Guid`. The server verifies that intent before consuming a nonce or invoking a
callback. Deployment prefixes do not change logical targets. Server-first continuations
require this binding too; they cannot rely on a later client RPC to authorize admission.

`SecureSession` stores the verified peer, selected name, server signer and weak
`RequestReplayCheck` outside shared seeds. Its context factory acquires the existing
nonce/clock owner only while a check runs. RPC and GUID admission call the same
`signature::accept_nonce`; no replay cache, clock snapshot or enrollment state is copied.
An unpolled check cannot keep the owner alive. WRT acquires both authentication and
continuation owners weakly.

The toolkit's connection-local context carries provenance into handlers.
`SignatureAuth` still checks current enrollment, roles, expiry, revocation and hooks on
every RPC. Request claims cannot replace the encrypted peer. Local/bearer credentials
retain their separate precedence. Setup, initialization and diagnostic contexts have no
session scope; HTTP resources retain their own authentication policy.

## Native clients and registry compatibility

`session::client::{admit,request_body,request}` share admission deadlines, captured JSON
bytes and response handling between Reqwest and the existing SOCKS connector. Each
connection prepares its own signed hello before the 30-second network deadline,
including opaque GPG signing. The standard signed timestamp freshness policy still
applies. The upgrade retains device-info headers, proxy/TLS policy, remote DNS and the
selected hostname rather than a pinned dial address. RPC upgrades do not redirect.
`reqwest-websocket` 0.5.0 shares the Tungstenite 0.26 message conversion; HTTP/1 upgrade
clients preserve ordinary HTTP client behavior separately.

Native keyed OS, registry, Tunnel and WRT calls use this transport. Local/bearer access
and missing-key public callers retain HTTP; malformed/read-permission/GPG failures fail
closed. CLI state/setup/init/diagnostic calls deliberately retain unsigned HTTP and legacy
GUIDs independently of stored keys. `CliWebSocketContext` selects fresh same-peer,
intended-GUID admission for normal keyed continuations. Existing consumers retain their
streaming, output, cancellation and keepalive ownership. WRT log/update commands return
GUIDs rather than adding native stream consumers; its HTTP RPC path remains `/rpc/v1`.

Backend registry discovery, cache misses and grouped subscriptions share the existing
reloadable SOCKS client. Upgrade HTTP 404/405/501 selects signed HTTP compatibility for
discovery and cache misses; the same registry task polls full/all-revision `package.get`,
`info` and the selected `os.index` immediately and every 15 minutes, retrying upgrade
support each cycle. Projection, selection and disabled-key discovery keep their owners;
polling grants no trust. Refresh/removal retain cancellation and join behavior. Successful
polling reports observed availability without a persistent socket. Redirects, authorization
errors, transport failures and post-upgrade admission errors never select that fallback.
On plain HTTP an active attacker can forge an absent-route response; HTTPS protects that
support-detection exchange. CLI administration retains its separate transport policy.

## Continuations, results and bounds

`RpcContinuation::{ws,ws_authed}` capture optional session provenance. Secure-origin GUIDs
require a fresh proof from the initiating identity for that exact GUID. A visible GUID
alone cannot release application data. Legacy-origin GUIDs retain their original
plaintext compatibility. One-shot redemption can still consume a known GUID on a failed
attempt; it does not grant the initiating identity.

`WebSocketFuture` owns revocation during both admission and callback. Public continuations
retain 1 KiB input and 15-second writes; authenticated continuations retain bulk input and
their 64 MiB allowance. Resource policy, revocation and encryption provenance remain
independent. `wire_message_size` and `wire_chunk_size` derive separate framing allowances;
close controls retain their independent 125-byte payload budget. A small request budget
does not constrain a 17 MiB dump or 5 MiB revision.

Native framing uses `LengthDelimitedCodec`. It accepts split/coalesced records, retains
bytes beyond the handshake and validates logical declarations before reserving bodies.
Retained prefixes expose header-only truncation without another progress counter. JSON
is not reserialized by the adapter, preserving numeric tokens, full-width IDs and BOMs.
The existing actor/wrapper owns backpressure, write deadlines and producer lifetimes.

Application close results are authenticated. Physical EOF or Close cannot establish
success, and a failed acknowledgement cannot discard an already verified result.
Update-progress termination is not business success: OS consumers consult staged restart
state. The generic `util/net.rs::WebSocket` retains keepalive and close ownership.

## Identity observations and browser caller contracts

Registry-administration CLI observations use atomic `registry-identities/` records beside
the configured identity path, scoped by sanitized selected URL/hostname/port/base path.
First contact is silent; replacements warn on stderr and are accepted, not pinned. Storage
and advisory-output failures cannot change JSON stdout, RPC results or exit status.
Normal OS/Tunnel/WRT native calls and backend OS registry connections do not own these
records or warnings.

Own-server browsers keep identity history atomically in the existing IndexedDB store,
separate from device keys and surviving logout. Replacements produce nonblocking notices;
foreign targets, startup and registry listings do not become own-server history. Trusted
delivery of replacement JavaScript remains outside this protection.

Browser adapters must use the native-generated profile and generic commitment signer and
verifier owners, without exporting private keys. A key/provider lease may persist; each
connection needs fresh DH/nonce/proof data. Existing RxJS refcounts, key-rotation/logout
cancellation, metadata fences and product reconnect/recovery owners remain authoritative.
A stale local-closing socket's error cannot reset its replacement; abnormal unverified
closure remains code 1006 with `wasClean: false`. Active transport loss still errors.

Normal keyed browser RPC and secure-origin GUIDs cannot downgrade from unauthenticated
state replies or failed admission. Startup/state/init/diagnostic callers deliberately use
their explicit legacy contracts. Resource HTTP headers, MCP and firmware metadata retain
their separate owners. Application encryption does not protect unrelated HTTP resources.
