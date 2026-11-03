# S9PK trailer fixtures

Both native and TypeScript tests consume these frozen files. `vectors.json` records their exact commitments, canonical public identities, and native-generated ph, pure, legacy-labeled ph, large-bound, and adversarial point signatures. All signing seeds are deterministic test data in the generator; none are production keys.

- `base.s9pk`: embedded signer only.
- `endorsed.s9pk`: three valid supplemental signers, without diagnostics.
- `mixed.s9pk`: the same endorsements, a PEM identity alias, an invalid cryptographic record, an unknown TLV, and unknown nested JSON fields with a large integer.
- `overlap.s9pk`: a fully verified archive whose unused image payload ends in a trailer-like footer. Its commitment differs from `base.s9pk`.

Every archive has a manifest, version, dependencies, icon, and license usable by the sideload preview. Image/JavaScript payloads are test sentinels, not installable service files.

The ignored native test `regenerate_s9pk_trailer_fixtures` regenerates the directory; run it explicitly through the cache-backed build tool and format `vectors.json` with the root Prettier configuration afterward.
