FROM node:24-bookworm-slim AS node

FROM start9/cargo-zigbuild

RUN apt-get update && \
    apt-get install -y --no-install-recommends squashfs-tools && \
    rm -rf /var/lib/apt/lists/*

COPY --from=node /usr/local/bin/node /usr/local/bin/node
