# Legacy storage fixtures

`legacy-795d8a265.tar.gz` contains real format-1, format-2 and format-3 stores generated from the production source at `795d8a265`, before caller-bound authentication was introduced. Migration tests must consume these bytes rather than serialize historical structures using the current definitions.

These are synthetic backups with deliberately public test passwords:

- `v1`: password `password`; root inode 1, raw packed record 42 and legacy blobs.
- `v2`: password `snapshots`; inode 2 contains `frozen-inline`, inode 3 contains 12,000 bytes of `0x42`, and inode 4 contains four chunks of `0x31`. Inode 3 has UID 123, GID 456, mode `06751` and xattr `user.fixture=original`.
- `v3`: the same files and password, with snapshots `before` and `after`. The first snapshot retains the original four chunks; the second includes `versioned` at chunk 1 plus offset 123. The live file is truncated to two chunks. The store includes stable legacy objects, immutable versioned objects and deletion overrides.

To regenerate from the frozen source in a build cache:

```sh
helix-build --output backup-fs-legacy-fixtures.tar.gz -- bash projects/start-os/backup-fs/testdata/generate.sh
mv backup-fs-legacy-fixtures.tar.gz projects/start-os/backup-fs/testdata/legacy-795d8a265.tar.gz
```

The generator restores the captured source before exiting. Encryption uses fresh random salts and nonces, so regeneration preserves plaintext and format contracts, not identical archive bytes.
