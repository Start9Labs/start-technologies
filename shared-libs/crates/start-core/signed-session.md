# Signed RPC session protocol, version 1

Identity authentication uses StartOS request commitments, `RequestProof::sign_commitment`,
`AnyVerifyingKey::verify_commitment` and `AnySignature`. Handshake proofs emit pure Ed25519
through `Signer::sign_message`; verification also accepts Ed25519ph. Existing package and
storage signature policy stays with its own signing owner. Client and server schemes
need not match. Adding a signing scheme extends those owners and the browser provider,
not the transport. WebAuthn enrollment, assertions and user-presence policy are separate.

## Admission

The bare `/rpc/v2` endpoint always requires encrypted admission. Deployment prefixes
remain supported. There is no plaintext bare-RPC mode or URL negotiation selector.
GUID continuations use `/rpc/v2/<guid>`; `/ws/rpc/<guid>` remains a compatibility alias.
Legacy-origin continuations retain plaintext compatibility;
secure-origin continuations require a fresh exchange from the initiating identity
for that exact GUID. HTTP authentication remains independent.

The server obtains allowed signing names from `SignatureAuthContext`'s existing
ingress policy. The request authority ranks an allowed candidate first; the verified
proof determines the matched name. Dial-IP and proxy aliases do not change the
client's selected signing identity. No query parameter selects a trusted context.

There are two binary handshake records. Each record has a two-byte big-endian length
and exactly two consecutive CBOR objects: a body followed by a structured proof.
The record has at most 1,024 bytes, with CBOR nesting bounded at 16 levels. Extra
objects, trailing data, duplicate/unknown fields and invalid binary lengths are
rejected. Maps use text keys; admission values are untagged. Binary fields are definite
CBOR byte strings, not Base64 or integer arrays. CBOR integers preserve the full
i64/u64 ranges without JavaScript-number rounding. Native and browser decoders validate
wire kinds through their existing CBOR libraries before DTO interpretation; buffered
decoding must not normalize an invalid representation into an accepted field. Browser
`decodeFirst` is asynchronous and returns the original first-object byte length.

Each body has `protocol: "Start-RPC-Session"`, `version: 1` and a `hello` map:

1. Client → server: `role: "client"`, `ephemeral` containing the 32-byte X25519 public
   key, and `target`. Ordinary RPC uses `"rpc"`; a continuation uses
   `{"continuation":"<guid>"}` with its existing GUID.
2. Server → client: `role: "server"`, its own `ephemeral`, and `clientHello` containing
   SHA-256 of the exact captured client record contents, excluding the length prefix.

The proof map contains:

- `commitment`: `timestamp` (i64), `nonce` (u64), `size` (u64), and `blake3` (32 bytes).
- `signer`: the existing algorithm-tagged DER verifying key as a byte string.
- `signature`: the existing algorithm-tagged DER signature as a byte string.

The request-commitment type is fixed by this protocol and its existing signature-domain
type ID. The proof signs the exact captured CBOR body bytes using existing
commitment-signature framing and the selected hostname as signing context. The
receiver retains the first object's byte boundary; it must not reconstruct signed
bytes by re-encoding a decoded map. No canonical-CBOR requirement or HTTP query/header
proof encoding is introduced. Binary adapters leave existing storage, package and
HTTP representations unchanged.

The receiver rejects invalid proofs, protocol/version/role and noncontributory DH.
The client also checks `clientHello` against the complete initiation it sent, binding
the server response to its identity, proof, ephemeral key and target.

The server checks the route-derived target and expected peer before nonce consumption
or callbacks. An RPC hello cannot authorize a continuation, or one GUID another.
This also applies to server-first streams. RPC and GUID admission share the existing
live `accept_nonce` clock/replay policy. `RequestReplayCheck` acquires its
`SignatureAuthContext` owner weakly, when polled; it must not retain the context,
copy clock/enrollment state or introduce a second nonce cache.

A valid proof from an unregistered key completes admission but grants no permissions.
It can call unauthenticated methods; protected methods check live registration and
authorization. Enrollment and revocation affect subsequent calls on the same open
connection. No callback receives unauthenticated plaintext as an admitted request.

Each physical connection gets fresh DH keys, commitment and nonce. Client signing
finishes before the 30-second network/admission deadline; provider handles may be
reused, prepared hellos may not. The existing signed-timestamp freshness policy still
applies while signing and connecting. There is no relaxed clock policy or hidden
signing retry. The server signs only after validating the initiation.

The first message exposes the client's public identity on `ws://`; `wss://` protects
it. Private signing keys never travel. Identity proof is separate from enrollment,
roles, and package/OS signing trust. Hooks and authorization remain live per call.

## Key derivation and records

X25519, HKDF-SHA256 and AES-256-GCM use existing maintained implementations. AES-GCM
retains interoperable native browser WebCrypto support. Key establishment and identity
authentication are classical; harvest-now/decrypt-later protection is outside this
protocol's threat model. Outer TLS can independently negotiate hybrid post-quantum
key establishment. The raw DH secret is never a record key.

Let `C` and `S` be the exact client/server record contents, including both CBOR objects:

```text
transcript = SHA256(u32be(len(C)) || C || u32be(len(S)) || S)
clientKey = HKDF-SHA256(DH, salt=transcript,
                      info="Start-RPC-Session v1\0client-to-server", length=32)
serverKey = HKDF-SHA256(DH, salt=transcript,
                      info="Start-RPC-Session v1\0server-to-client", length=32)
```

Each direction has a separate key and counter, initially zero. No counter or keystream
offset is transmitted. The 12-byte AES-GCM nonce is four zero bytes followed by the
u64 big-endian counter. AAD is `Start-RPC-Session v1\0` followed by direction byte
zero for client-to-server or one for server-to-client. Each record carries ciphertext
followed by its 16-byte tag, with a two-byte big-endian record length. Counters advance
per encrypted record, not per WS frame. Keys admit at most 2^32 records; exhaustion
closes the connection before nonce reuse. Authentication failure terminates the
channel rather than attempting resynchronization. Replays, reflection and reordering
cannot authenticate as the next record.

Ephemeral secrets are discarded after deriving directional keys. The first valid
encrypted client record confirms possession of its DH secret. A replayed signed hello
alone cannot dispatch an RPC. There is no additional ready/ack exchange.

Encrypted records have at most 65,535 bytes; plaintext chunks have at most 65,519.
The decrypted byte stream uses a four-byte logical length followed by the existing
text/binary/close kind byte. Exact JSON bytes, numeric precision and full-u64 RPC IDs
remain intact. Request, response and close budgets are independent.

Binary WS data feeds the record decoder; plaintext WS text is a protocol error.
Physical Ping/Pong remain transport controls and consume no crypto counter. Physical
Close/EOF cannot establish application success. Native legacy continuation close
outcomes remain authenticated, and a failed acknowledgement cannot erase a verified
result. Frontends treat active transport closure uniformly through their reconnect
owner; `rpcstream` terminal events, not WS close codes/reasons, carry application
completion/errors. Socket loss cancels every subscription owned by that socket;
reconnecting creates fresh subscriptions. Disposal/logout do not create replacement
connections without consumers.
