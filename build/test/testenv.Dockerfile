# start-core tests that exercise s9pk packing need the same Node.js and
# SquashFS tools start-cli invokes in production.
FROM start9/cargo-zigbuild

RUN apt-get update \
    && apt-get install -y --no-install-recommends nodejs squashfs-tools \
    && rm -rf /var/lib/apt/lists/*
