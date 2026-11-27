# Hybrid torrent fixture

`base-hybrid.torrent` describes the existing `../s9pk-trailer/base.s9pk` fixture using both BEP 3 and BEP 52. Torrenttools 0.6.2 generated it independently of librqbit; its v1 and v2 verification modes both report 100% for that payload.

From the repository root, with `torrenttools` on PATH:

```bash
torrenttools create --protocol hybrid --piece-size 16K \
  --no-creation-date --no-created-by --no-cross-seed \
  --output shared-libs/crates/start-core/testdata/bittorrent/base-hybrid.torrent \
  shared-libs/crates/start-core/testdata/s9pk-trailer/base.s9pk

torrenttools verify --protocol 1 \
  shared-libs/crates/start-core/testdata/bittorrent/base-hybrid.torrent \
  shared-libs/crates/start-core/testdata/s9pk-trailer

torrenttools verify --protocol 2 \
  shared-libs/crates/start-core/testdata/bittorrent/base-hybrid.torrent \
  shared-libs/crates/start-core/testdata/s9pk-trailer
```

- V1 infohash: `aa7225bd6288e39b146b3aeb18d4846d9a4b011a`
- V2 infohash: `8485f107f399acd3087f4043f4075bd7ca878ffbe18eb41a64b2d86669545762`

Tests add a loopback tracker outside the hashed info dictionary and exercise metainfo downloads and a dual-hash magnet through the v1-compatible swarm. Torrenttools is needed for regeneration, not for running the tests.
