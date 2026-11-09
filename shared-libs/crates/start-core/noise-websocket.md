# Noise WebSocket adapter

`util/net/secure.rs::EncryptedSocket` composes the shared `sign/noise.rs` identity
primitive with a WebSocket message transport. Snow owns the XX handshake,
DH/HKDF, transcript, transport nonces and ChaCha20-Poly1305. Identity certificates
use the standard libp2p Noise profile; no additional transcript signature is used.

The adapter does not enable encryption on a route by itself. A
`rpc_toolkit::WebSocketUpgrader` can admit it before the RPC actor starts. Legacy
continuations can retain `util/net.rs::WebSocket` around it: that generic wrapper
still owns their keepalive, send deadline and close lifecycle. The cipher adapter
adds no parallel heartbeat or producer task.

## Identity ownership

`sign/instance.rs` owns private server-key persistence for registry, Tunnel and
WRT contexts. Each loads `rpc-identity.pem` from its persistent directory. Creation
uses `AtomicFile::save_no_replace` with a unique temporary path: concurrent starts
read the same completed winner rather than replacing one another's identity.
Existing invalid keys fail startup without rotation. Files use mode `0600`, and
both contents and directory metadata are synced before return. WRT's sysupgrade
keep list includes its identity; in-memory test contexts use ephemeral keys.
StartOS's account already owns its developer signing identity.

This persistence is independent of enrolled client keys, authcookies, package
signer trust, and networking/TLS keys.

## Connection scope and negotiation

`middleware/auth/session.rs::RpcSessionUpgrader` selects Noise with
`noise=1&context=<server-name>`. Missing opt-in retains legacy RPC authentication;
unsupported versions, failed exchanges and invalid contexts fail without a
plaintext fallback. The selected name must pass the existing `request_identities`
ingress/loopback policy. `prologue` binds that name to the Noise transcript.

After completion the upgrader installs opaque `SecureSession` provenance on a
clone of the actual RPC context, outside its shared seed. `SessionContext` exposes
that provenance; setup, initialization and diagnostic contexts remain unscoped.
The toolkit's connection-local factory supplies it to handlers and middleware.
`SignatureAuth` uses the peer identity instead of per-message proofs while still
running live `check_pubkey`, `post_auth_hook` and password-enrollment rate limits.
Request-supplied proofs cannot switch the identity of an encrypted connection.
Local/bearer authentication retains its separate precedence and policy.

StartOS's normal RPC context, Registry and Tunnel register the shared upgrader route on `/ws/rpc`. The native registry client uses
`session_url` to serialize the shared negotiation type and version, then binds
the same selected hostname through `prologue`. Configured registry hostnames must
cover that name. Other product routes opt into the upgrader explicitly.

`util/net/tungstenite.rs` adapts native socket messages to the shared transport;
Noise and test clients use that same bridge. Native registry connections retain
the existing SOCKS routing, remote DNS, heartbeat, retry and cancellation owner.
Identity preparation precedes the 30-second connection/exchange deadline. No RPC
initiation is sent before admission completes; failed negotiation does not retry
as plaintext. Package signatures remain the StartOS trust anchor.

Uncached backend registry RPC and initial registry-info/trust discovery share
`registry::context::call`, using the same SOCKS-aware upgrade as subscriptions.
Device-info headers accompany the upgrade for existing compatibility projection.
`session::client::{admit,request_body,request}` own admission deadlines, captured
request bytes and single-response exchange for both Reqwest and SOCKS transports.
Initial package-trust grants, refresh discovery and cache projections retain their
distinct owners; no transport identity is persisted in the registry cache.

A registry upgrade route returning HTTP 404, 405 or 501 selects legacy signed HTTP
for discovery and uncached backend calls. The existing registry task immediately
polls `package.get` with full version details, `info` and the selected `os.index`,
then repeats every 15 minutes. It rechecks WebSocket support each cycle. Polling
reuses the snapshot projection and trust-discovery owners; it grants no trust.
Refresh/removal retain their existing cancellation and join behavior. Successful
polling reports availability without a persistent socket. Redirects, authorization
errors, transport failures and rejected encrypted admission do not select this
fallback. Native registry administration and other products retain their existing
transport policy.

## Native CLI transport

Normal keyed StartOS, registry, Tunnel and WRT CLI calls, including registry promotion, use
`session::client` with the same
opaque signer, session negotiation and encrypted socket as native subscriptions.
Serialization precedes certificate preparation; GPG-agent unlocking and signing
finish before connection/exchange deadlines. A refused admission never retries as
plaintext. Local/bearer access and public calls without an initialized identity
retain HTTP. Malformed keys and failed GPG-agent resolution or signing fail
closed.

`CliContext` parses proxy and TLS policy once for its ordinary HTTP client and a
separate HTTP/1-only, nonredirecting upgrade client. The upgrade client retains
legacy WebSocket `--insecure` hostname handling; ordinary HTTP settings are unchanged.
`reqwest-websocket` 0.5.0
retains Reqwest's routing and trust policy while sharing the existing
Tungstenite 0.26 message conversion. Logical responses use the shared 64 MiB
limit; framing overhead stays separate. Versioned RPC URL conversion preserves
deployment prefixes and queries. `RpcSessionUpgrader::url` owns the common
WebSocket endpoint derivation; versioned HTTP paths use `util::net::rpc_url`.
CLI `state`, setup, initialization and diagnostics explicitly retain unsigned HTTP
and legacy GUIDs independently of stored keys. Diagnostic local-token authorization
remains separate. `CliWebSocketContext` selects fresh same-peer Noise admission for
normal keyed GUIDs and legacy admission for startup contexts. Original selected
names survive `.local` dial-address pinning. Both use the configured upgrade client;
existing native consumers retain streaming, cancellation and output ownership.
The existing generic `WebSocket` owns continuation keepalives and 15-second writes;
no additional connection task or reconnect loop is added.

Native result consumers distinguish progress termination from the final application
Close outcome. Empty close/EOF cannot establish success; an already verified Close
result survives a failed subsequent acknowledgement. Normal update progress derives
its outcome from the database's staged-update restart state. WRT CLI shares the
signing context's automatic credential selection and returns log/update GUIDs to
separate consumers. Its selected HTTP target is `/rpc/v1`; the shared converter
preserves its deployment prefix.

Keyed registry CLI commands remember verified server identities in
`registry-identities/` beside the configured CLI identity path (by default,
`~/.startos/registry-identities/`), including when an opaque GPG identity is selected.
Each sanitized target URL has one atomic PEM record, scoped by selected hostname,
port and deployment path, not an mDNS-pinned dial IP. First contact and unchanged
keys are silent. Rotation writes target/previous/current fingerprints and
out-of-band verification advice to stderr, then remembers the new verified key.
Fingerprints are SHA-256 of raw Ed25519 public bytes, displayed as `SHA256:` plus
lowercase hexadecimal. Storage and advisory-output failures do not change RPC
results, stdout or exit status. Registration/permissions remain live and separate.

The registry-specific CLI flow observes the peer after completed admission and
before sending the RPC, including calls subsequently denied authorization. Normal
OS/Tunnel/WRT native calls, OS registry discovery/subscriptions, local-token and
anonymous HTTP calls do not own these records or warnings.

## Browser transport

The binding generator emits `noise-profile.ts` from the native identity, session
and framing owners. It carries the negotiation template, prologue prefix, endpoint,
kind values, framing limits, allowed close-code ranges and identity-fingerprint profile. Angular infrastructure
imports this data and maintained `@libp2p/noise` 17.0.7; the SDK has no Noise
dependency. The library owns DH, transcript, nonces, certificates and AEAD. An
opaque WebCrypto signer supplies a prepared standard identity signature without
exporting its Ed25519 private key. Preparation precedes connection/exchange and
RPC-response deadlines. Prepared DH identities live with the active device-key
lease, independently of persistent enrollment and per-exchange ephemeral keys.

`RpcSocketService` multiplexes active calls through the existing RxJS WebSocket
subject and redeems encrypted continuations through fresh exchanges. The last
unsubscribe closes the subject's socket; no separate heartbeat or reconnect
manager is added. Device-key replacement, rollback and logout cancel active
channels. Captured request bytes are not reparsed to add correlation IDs. StartOS and Tunnel use this transport when a device key exists; missing keys
retain anonymous public HTTP, never negotiation-failure fallback. Their PatchDB
sources subscribe only after authentication. StartOS initialization, diagnostics
and state/DNS probing use a separate `StartupApiService` contract: unsigned HTTP
and legacy-origin WebSocket continuations, independent of any stored key. An
unauthenticated state response cannot downgrade a normal call or its continuation.
The existing state observer retains navigation, retry and DNS sequencing; no
mode snapshot, copied method allowlist or global per-key security flag is added.
`session_rpc_router` composes the common OS/Tunnel routes; MCP stays an independent
OS route. Setup/init/diagnostic contexts remain unscoped. StartWRT uses the same browser
transport for keyed RPC and DOM-style GUID sockets. Its existing reconnect/update
owners abort a stalled shared session before bounded recovery probes; anonymous
bootstrap retains aborting HTTP. Typed firmware metadata and the retained HTTP
header feed its existing stale-UI detector. Maintained Noise/stream modules load lazily;
identity signing still finishes before native connection and RPC-response deadlines.

Completed browser admission carries the verified raw public key in `NoiseOpenEvent`.
`ServerIdentityService` fingerprints it using the Rust-generated profile and atomically
compares/replaces one record in the existing identity IndexedDB store, separately from
the device key. The app's key namespace and same-origin API path scope this record;
logout removes only the device key. First contact and unchanged keys are silent. A
rotation lazily opens a nonblocking Taiga notice with verification advice and fingerprint
details; it remembers the newly verified key rather than pinning the old one. RPC and
continuation open observers keep their existing ownership. Storage failure does not
reject RPC or claim that the key was remembered. Foreign API targets, startup and
registry listings never become own-server history. This warning does not protect
replacement JavaScript delivered through a compromised origin.

The browser adapter composes libp2p's message stream and length-prefixed stream
with a WebSocket-compatible facade. Its logical receive and queued-write payload
allowances are 64 MiB, with 15-second writes. Bounded raw chunks also avoid the
maintained Noise decoder's fixed 16 MiB ciphertext-buffer limit. DOM WebSockets
provide neither read pausing nor a drain event; a libp2p repeating task observes
`bufferedAmount` only while backpressured and stops on close. The maintained
WebSocket transport does not export its URL-level stream adapter; constructing
its multiaddr transport would introduce unrelated dialing/upgrading ownership.
Application close status/reasons remain encrypted, and physical close events
cannot fabricate authenticated success.

## Admission

`initiate` and `accept` return the verified peer identity only after the local
Noise state enters transport mode. Prepare `NoiseIdentity` before calling them;
interactive GPG signing stays outside the 30-second exchange deadline. Handshake
records have a 1,024-byte wire-payload allowance, excluding their length prefix. Message one must have an empty payload. The application supplies
identical, cryptographically bound prologue bytes on both ends.

`accept` can require a particular peer identity, independently of enrollment or
roles. This is the primitive for same-peer continuation redemption; the caller
owns recording and supplying that requirement. Cryptographic admission does not
grant RPC privileges. Live enrollment, revocation and method authorization remain
application responsibilities, using the admitted connection context.

## Wire framing

All lengths are big-endian. `tokio_util::codec::LengthDelimitedCodec` owns native
framing; the browser uses libp2p's length-prefixed stream with generated widths.

| Layer                                   | Length field | Content                                                         |
| --------------------------------------- | ------------ | --------------------------------------------------------------- |
| Noise record                            | 2 bytes      | Handshake message or transport ciphertext, at most 65,535 bytes |
| Logical message, inside Noise plaintext | 4 bytes      | Kind byte followed by original payload bytes                    |

`secure.rs::MessageKind` owns the kind values: `0` is UTF-8 text, `1` is binary and
`2` is a close result. The logical length includes the kind byte. The configured
application receive budget counts text/binary payload alone. Close results retain
the independent WebSocket control allowance of 125 bytes: an empty payload means
no status, otherwise a two-byte status code precedes a UTF-8 reason of at most
123 bytes. Reserved status codes, malformed UTF-8 and oversized close results are
rejected. The control allowance does not enlarge the text/binary budget.

JSON is not decoded or serialized by this adapter, preserving original numeric
tokens and full-width request IDs.

Transport plaintext is split into records of at most 65,519 bytes. A complete
outgoing record, including its prefix, is a binary WebSocket message of at most
65,537 bytes. Receive framing accepts split prefixes, split records, coalesced
records and logical messages spanning records; WebSocket message boundaries do
not delimit logical application messages. Bytes read beyond the last handshake
record remain available to the transport decoder.

An application close sends its encrypted result before an empty physical Close
frame. The receiver acknowledges through the existing sink before delivering the
authenticated logical Close, retaining its status and reason for `close_result`
and continuation consumers. A failed transport acknowledgement does not discard
a result that was already authenticated. The deferred close result lives only
until acknowledgement or terminal delivery, without a separate task or heartbeat.
The existing socket wrapper still owns send and close deadlines.

Ping and Pong retain ordinary WebSocket representation. Physical Close controls
can terminate the transport, but their unauthenticated status/reason never become
an application result. Plaintext data frames, invalid kinds/UTF-8, failed
authentication and truncated messages fail the channel. There is no plaintext
fallback.

## WebSocket continuations

`RpcContinuation::ws` and `ws_authed` capture optional `SecureSession` provenance
from their issuing request. A secure-origin GUID requires a fresh exchange with
the same client identity and server-name prologue, regardless of redemption URL
claims. The stored provenance holds keys and name, not the RPC context or its
background resources. A visible GUID alone cannot release application data.
Legacy-origin GUIDs retain plaintext compatibility even when the same key also
uses an encrypted connection. One-shot redemption still permits denial of service
by consuming a visible GUID; it does not grant the initiating identity.

The existing `WebSocketFuture` revocation owner wraps both admission and callback,
including a stalled handshake. Resource policy remains independent: public
payloads are limited to 1 KiB with 15-second writes; authenticated payloads retain
the 64 MiB message allowance and bulk-write policy. Encrypted wire chunks use
`wire_chunk_size`; framing overhead does not expand the plaintext budget. The
same generic `WebSocket` wrapper owns both transports' keepalive and close flow.
REST continuations and other HTTP resources retain their existing HTTPS policy.

## Bounds and ownership

`wire_message_size` derives the raw receive allowance from the logical limit,
framing/tag overhead and handshake allowance. The logical decoder validates the
declared length before reserving its body. Retained prefixes make header-only
truncation observable without a second progress counter. The toolkit also checks
logical message size after adaptation, before parsing JSON.

Outgoing logical messages are independent of the incoming request limit. A
1 MiB request allowance does not restrict a 17 MiB snapshot or 5 MiB revision.
The cipher sink accepts one logical message at a time and drains its bounded-size
records through the underlying sink's backpressure; the existing actor/wrapper
owns the write deadline. No producer or subscription tasks are added.

Application encryption does not protect unrelated HTTP resources or replace
trusted delivery of browser JavaScript.
