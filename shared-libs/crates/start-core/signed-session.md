# Signed RPC session protocol, version 1

This contract defines algorithm-independent identity authentication for an
application-encrypted WebSocket. Identity proof uses StartOS request commitments,
`Signer::sign_commitment`, `AnyVerifyingKey::verify_commitment` and `AnySignature`.
The identity schemes on the two sides need not match. The server uses its existing
Ed25519 identity initially. Adding a signing scheme extends those owners and the
browser signing/verifying provider, not this exchange. This does not itself
implement WebAuthn enrollment, assertions or user-presence policy.

## Admission

The client selects `/ws/rpc?secure=1&context=<selected-hostname>`, retaining any
deployment prefix. The server validates the selected hostname through its existing
ingress/signing-identity policy. Missing opt-in retains the deliberately selected
legacy transport; unsupported versions and failed encrypted admission fail closed.

There are two binary handshake records. Each record has a two-byte big-endian
length and a JSON envelope with exactly `payload` and `proof` fields:

- `payload` is standard Base64 of the complete UTF-8 message body.
- `proof` uses the existing request-proof query encoding: request commitment,
  timestamp, nonce, size, BLAKE3 digest, algorithm-tagged DER verifying key and
  algorithm-tagged DER signature. Full-width integers stay decimal strings in this
  encoding, never JavaScript JSON numbers.

Each body has `protocol: "Start-RPC-Session"`, `version: 1` and a `hello` object:

1. Client → server: `hello` has `role: "client"` and `ephemeral`, the Base64-encoded
   32-byte X25519 ephemeral public key.
2. Server → client: `hello` has `role: "server"`, its own `ephemeral`, and
   `clientHello`, the Base64-encoded SHA-256 digest of the exact client envelope
   bytes, excluding the record length prefix.

The proof signs a `RequestCommitment` over the exact decoded payload bytes using
existing commitment-signature framing and the selected hostname as signing
context. There is no parallel transcript-signature format or canonical-JSON
algorithm. Protocol, version and role are signed body data. A peer rejects the
wrong role/version/protocol, malformed proof or body, and a noncontributory X25519
exchange. The client additionally checks `clientHello` against its original
message. This binds the server response to the client's identity, proof and DH key.

The receiving server verifies the client proof and exact body before applying the
existing `accept_nonce` clock/replay policy. RPC admission and secure-origin GUID
redemption use the same policy owner. A GUID also requires the original client
identity. Replay checking must not retain the RPC context, cache clock/enrollment
state or introduce a second nonce cache. `SignatureAuthContext` owns that policy;
`RequestReplayCheck` acquires it weakly through `SecureRpcContext::replay_check`.
Creating an unpolled check must not keep its authentication owner alive.
No RPC callback receives unverified data.

Each connection gets a fresh client ephemeral key, commitment and nonce. Client
signing finishes before the network/exchange deadline; identity-provider handles
may be reused, prepared connection hellos may not. The server signs its response
after receiving and validating the client hello. Both handshake records retain a
1,024-byte payload allowance and a 30-second admission deadline.

Unlike Noise XX, the first message exposes the client's public identity on
`ws://`; `wss://` protects that handshake. Private signing keys are never exported.
A valid identity proof does not enroll a client, grant roles or establish package
signing trust. Method authorization, hooks and revocation remain live.

## Key derivation and records

X25519, HKDF-SHA256 and AES-256-GCM use maintained standard implementations already
used by StartOS. The raw DH secret is never a record key.

Let `C` and `S` be the exact client and server envelope bytes. Compute:

```text
transcript = SHA256(u32be(len(C)) || C || u32be(len(S)) || S)
clientKey = HKDF-SHA256(DH, salt=transcript,
                      info="Start-RPC-Session v1\0client-to-server", length=32)
serverKey = HKDF-SHA256(DH, salt=transcript,
                      info="Start-RPC-Session v1\0server-to-client", length=32)
```

Each direction has its own key and counter, initially zero. An AES-GCM nonce is
four zero bytes followed by the eight-byte big-endian counter. AAD is the bytes
`Start-RPC-Session v1\0` followed by the direction byte: zero for client-to-server,
one for server-to-client. A record carries ciphertext followed by its 16-byte
GCM tag. A key admits at most 2^32 records; exhaustion closes the channel before
nonce reuse. Authentication failure terminates the connection. Replays, reflection
and reordering cannot be accepted as the next authenticated record.

Ephemeral secrets are discarded after deriving the directional keys. The first
valid encrypted client record confirms possession of the DH secret; a replayed
signed hello alone cannot cause an RPC to dispatch. There is no additional
application-level handshake acknowledgement.

Existing framing and lifecycle owners remain authoritative: encrypted records have
two-byte lengths and at most 65,535 bytes; plaintext chunks have at most 65,519
bytes. The decrypted byte stream uses a four-byte logical length followed by the
existing text/binary/close kind byte. Request, response, close and public-stream
budgets remain independent. Exact JSON bytes, numeric precision and full-width
request IDs are retained. Application close outcomes are authenticated; physical
EOF/Close cannot claim success or erase a verified result. Existing task, actor,
RxJS, heartbeat, backpressure, cancellation and revocation owners remain in charge.
