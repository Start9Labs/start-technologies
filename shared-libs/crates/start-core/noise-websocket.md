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

Product routes opt into this upgrader explicitly; the adapter and scope alone do
not change an existing route.

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

All lengths are big-endian. `tokio_util::codec::LengthDelimitedCodec` owns both
framing layers.

| Layer                                        | Length field | Content                                                         |
| -------------------------------------------- | ------------ | --------------------------------------------------------------- |
| Noise record                                 | 2 bytes      | Handshake message or transport ciphertext, at most 65,535 bytes |
| Logical data message, inside Noise plaintext | 4 bytes      | Kind byte followed by original payload bytes                    |

Kind `0` is UTF-8 text and kind `1` is binary. The logical length includes the kind
byte. The configured application receive budget counts the payload alone. JSON
is not decoded or serialized by this adapter, preserving original numeric tokens
and full-width request IDs.

Transport plaintext is split into records of at most 65,519 bytes. A complete
outgoing record, including its prefix, is a binary WebSocket message of at most
65,537 bytes. Receive framing accepts split prefixes, split records, coalesced
records and logical messages spanning records; WebSocket message boundaries do
not delimit logical application messages. Bytes read beyond the last handshake
record remain available to the transport decoder.

Ping, Pong and Close retain ordinary WebSocket control semantics and wire
representation; they are not encrypted application data. Plaintext data frames,
invalid kinds/UTF-8, failed authentication and truncated data messages fail the
channel. There is no plaintext fallback.

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
