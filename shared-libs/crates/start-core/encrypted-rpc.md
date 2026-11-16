# Encrypted RPC integration

`util/net/secure.rs::EncryptedSocket` composes the [signed-session protocol](signed-session.md)
with the existing WebSocket transport. `sign/session.rs` owns identity proofs, ephemeral
DH, key derivation and record authentication. The adapter owns framing, not another actor,
heartbeat or reconnect task.

## Identity and trust

`sign/instance.rs` persists registry, Tunnel and WRT identities in their existing
PatchDBs as `serverIdentityKey`. PatchDB's idempotent transaction commits a missing identity
before admission starts; concurrent initializers retain the committed winner. Malformed
stored keys fail startup without rotation. Identity persistence uses no separate file;
PatchDB files use mode `0600`. WRT preserves its authentication database during sysupgrade. Registry public index feeds
and WRT session APIs exclude this field. Tunnel's existing authenticated full-database
feed includes it alongside its WireGuard and TLS secrets. Registry and Tunnel admin root
`db.dump` results also include it. Copies can bypass advisory fingerprint-change detection;
revoking a session does not revoke a copied server key. StartOS signs with its existing
account `server_identity_key`; its persisted `developerKey` field and historical backup
formats retain their names. Startup contexts retain temporary identities. Server identity is
separate from enrollment, authcookies, package/OS trust and TLS keys.

`sign/fingerprint.rs` owns identity fingerprints: SHA-256 of raw Ed25519 public bytes,
formatted as `SHA256:` followed by lowercase hexadecimal. Extending key schemes also
requires extending that owner; transport code does not impose a signing scheme.

## Admission and authorization

`middleware/auth/session.rs::RpcSessionUpgrader` requires encrypted admission on bare
`/rpc/v2`. The signed CBOR body carries the protocol and version; URL selectors cannot
choose plaintext mode or a signing identity. `request_identities` supplies allowed ingress
and loopback names, ranking the preserved Host authority first. Signature verification
selects a matching permitted identity, preserving logical names through IP pinning.

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

On Linux, `init::check_time_is_synchronized` reads the kernel with query-only `adjtimex`, matching systemd's `NTPSynchronized` maximum-error policy. Startup admission rereads this source without spawning a subprocess or caching a clock snapshot. Boot waiting, normal-context synchronization updates and root-CA dating share that owner.

The toolkit's connection-local context carries provenance into handlers.
`SignatureAuthContext` owns admission capabilities independently of database availability.
`SignatureAuthorizationContext` owns database-backed enrollment, permissions and key lifecycle.
A valid unknown key completes admission without roles. `SignatureAuth` checks current
enrollment, roles, expiry, revocation and hooks on every RPC, including enrollment or
revocation on an already-open connection. `auth::continuation_subscription` registers the existing revocation listener before rechecking current key, ephemeral-key or bearer-token membership. Typed, WebSocket and REST bindings consume that same listener; revocation during asynchronous preparation cannot create a fresh unrevoked continuation. `LoginContext` owns shared membership selectors, preserving each context's expiry policy and independently authenticated local credentials. No roles or database snapshot are retained by the listener. Request claims cannot replace the encrypted peer. Local/bearer credentials
retain their separate precedence. Setup, initialization and diagnostic seeds own ephemeral
proof identities and the shared bounded replay policy, with immutable session provenance
on context clones. These possession identities grant no enrollment roles. Their proof names
come from the existing validated local-host ingress policy. HTTP resources retain their own
authentication policy.

Startup HTTP retains its JSON-body gate without permissive CORS. WebSocket upgrades require
same-origin browser requests; Origin-less native callers remain valid. The dev proxy rewrites
its trusted upstream Origin alongside Host. `WebServer`'s router watch scopes upgraded RPC
reads to the selected epoch; router replacement ends pending admission and active/pending
operations through the existing transport lifecycle. The standard `take_until` adapter
carries this lifetime without a second socket manager or forwarding task.

Setup/attach/CIFS/recovery password inputs use `PasswordType::decrypt_in_session`:
unwrapped strings require completed encrypted admission; legacy JWE uses the existing
P256 decoder on HTTP or encrypted RPC. Normal auth/login retains its independent legacy
HTTP policy. Native and browser setup callers send strings inside the encrypted session,
without fetching or adding a password envelope. Invalid envelopes reject before accepting
setup work or mounting a backup; absent passwords and whitespace keep their existing
meaning. `PasswordType` and CIFS diagnostic formatting redact passwords. Legacy HTTP restore targets still serialize their CIFS credential fields as strings;
upgraded clients protect the whole RPC with encrypted admission.

## Native clients and registry compatibility

`session::client::SessionClient` establishes connections lazily inside `call()` and
retains them with the CLI context. Its immutable HTTP/1 client owns network policy;
endpoint, selected signing name, signer and upgrade headers scope reuse. The existing
`util::rpc_client::RpcClient` dispatches correlated responses through separate Unix-line
and encrypted-WebSocket adapters. Its idle reader drives the existing WebSocket keepalive.
Dropping an ordinary WebSocket call or stream sends its per-ID cancellation through the
existing reader; late replies cannot settle a canceled call or affect its siblings. The
server drops the pending handler future before any following response write. Native callers
set per-request deadlines with `tokio::time::timeout`; browser calls accept `timeout` in
milliseconds. These deadlines cancel the same ID and do not impose a default registry reply
deadline. Socket loss drops every pending handler future. Accepted application-owned work
and external side effects retain their independent lifetimes. Application errors retain
healthy sockets; transport failure invalidates them without replaying sent calls.
The Unix pool likewise never retries a possibly executed request.

`SessionClient::stream<Item, Final>` uses the same dispatcher for versioned encrypted
sessions. It returns initiating data/metadata separately from the typed item receiver;
`items.next()` propagates terminal errors and `items.finish()` returns the real successful
terminal value. Receiver/future drop cancels its ID, including during initialization.
Native queues share the 64 MiB response budget across serialized UTF-8 bytes and per-item storage, with a separate terminal slot. Backlog bursts do not fail solely on item count, and the reader never waits for a consumer.
Browser queues retain backlogs behind initiating PatchDB fences within the serialized UTF-8 response budget, without a scheduler-dependent item cap.
Application terminals drain accepted items; physical loss discards unfinished queues.
Canceled IDs remain in the response-slot map until their close frames are written or the
transport ends.
The existing owned reader polls cancellation writes concurrently with incoming traffic;
a blocked writer cannot suspend replies or keepalive reads. No per-call task or second
connection manager owns cancellation. Container/effects Unix-line calls retain their
ordinary reply contract; this adapter does not silently switch transports.

Backend discovery keeps its separate short-lived workflow through
`session::client::{admit,request_body,request}`, sharing admission and framing rather than
a second persistent manager. Handshake proofs emit pure Ed25519; verification retains
Ed25519ph compatibility. Each connection prepares its own signed hello before the 30-second network deadline,
including opaque GPG signing. The standard signed timestamp freshness policy still
applies. The upgrade retains device-info headers, proxy/TLS policy, remote DNS and the
selected hostname in both proof scope and HTTP authority rather than a pinned dial address.
The endpoint port survives that authority selection. RPC upgrades do not redirect.
`reqwest-websocket` 0.5.0 shares the Tungstenite 0.26 message conversion; HTTP/1 upgrade
clients preserve ordinary HTTP client behavior separately.

Native keyed OS, registry, Tunnel and WRT calls use this transport. Ordinary local/bearer
and missing-key public calls retain HTTP; malformed/read-permission/GPG failures fail
closed. Typed StartOS CLI update progress uses encrypted sessions with the same independent
local credential when present. A client-lifetime ephemeral proof identity admits that
transport without reading or persisting a device key; it grants no credential roles.
Initial authorization rejection retains the existing interactive login workflow; an item
or terminal error never replays the operation. `--no-progress` retains ordinary asynchronous
update initiation. CLI state/setup/init/diagnostic calls use a client-lifetime ephemeral proof
identity independently of stored keys; diagnostic local credentials retain their own
precedence. `CliWebSocketContext` selects fresh same-peer, intended-GUID admission for
startup and normal keyed continuations. Existing consumers retain their
streaming, output, cancellation and keepalive ownership. WRT log/update commands return
GUIDs rather than adding native stream consumers; its HTTP RPC path remains `/rpc/v1`.

Backend registry discovery, cache misses and grouped subscriptions share the existing
reloadable SOCKS client. A legacy HTTP 200 without upgrading, or upgrade HTTP 404/405/501, selects signed HTTP compatibility for
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
Typed RPC terminals carry actual operation outcomes; percentages and staged restart state
do not substitute for them. The generic `util/net.rs::WebSocket` retains keepalive and close
ownership.

Both initialization boot paths publish their actual result, then retire init observation
before replacing the router. Accepted typed observers retain a lease through the existing
connection actor's terminal flush or cancellation. Redeemed legacy GUIDs retain a lease
through their existing progress/close handling; unopened GUIDs do not delay retirement.
Retirement rejects new init subscriptions and drains accepted leases within the default RPC
write deadline. A stalled initial batch can otherwise prevent terminal delivery before a
write even begins. Deadline expiry permits normal epoch cancellation; no terminal is
guaranteed across peer loss, reboot or process replacement. Outcome replay remains available
while an init context is active, before retirement.

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

Browser adapters use native-generated profiles and hello types, with `auth.RequestSigner`
and `auth.verifyRequestProof` owning proof creation/verification outside the transport.
The handshake module loads lazily and uses the maintained Noble implementations of X25519,
HKDF-SHA256 and AES-256-GCM on both HTTP and HTTPS. Browser randomness comes from
`crypto.getRandomValues`; the existing UUID library supplies correlation IDs without requiring
secure-context `crypto.randomUUID`. Persisted enrollment keys remain nonextractable WebCrypto
keys. Missing-key startup admission uses an opaque, unpersisted Ed25519 `RequestSigner`, sharing
the existing commitment/proof owner rather than an enrolled-key shape. Setup/state/init/diagnostic
browser calls and GUIDs use that admission lease; their peers never enter normal-product advisory
history. Normal product API owners attach verified-open observers, including on healthy reuse
and authorization rejection. A key/provider lease may persist; each
connection needs fresh DH/nonce/proof data. Existing RxJS refcounts, key-rotation/logout
cancellation, metadata fences and product reconnect/recovery owners remain authoritative.
A stale local-closing socket's error cannot reset its replacement. Frontend stream success
and errors belong to `rpcstream` terminal events, not WebSocket close codes or reasons.
Active transport closure follows reconnection policy; disposal must not create replacement
sockets. Socket loss cancels every pending initialization and active subscription it owns.

Start-core's `with_cli_progress` renders typed `FullProgress` items through the existing
indicatif renderer and returns the real terminal value. The shared progress source observes
the independently owned operation task, not percentage or socket closure. Dropping an
update or sideload observer does not abort accepted work. Sideload completion includes
installation finalization and uploaded-archive cleanup. Its CLI uploads concurrently with
observation; failure drops the other client-side future without replay. Typed sideload initiation
returns the upload capability without allocating a redundant progress GUID. Ordinary calls
retain that progress GUID. Legacy GUID consumers retain their wire format while sharing
that completion owner. Authenticated observers retain the
signer's revocation lifetime.

Normal keyed browser RPC and secure-origin GUIDs cannot downgrade from unauthenticated
state replies or failed admission. Startup/state/init/diagnostic callers use
their explicitly selected transport contracts, with legacy HTTP callers still supported.
Resource HTTP headers, MCP and firmware metadata retain
their separate owners. Application encryption does not protect unrelated HTTP resources.
