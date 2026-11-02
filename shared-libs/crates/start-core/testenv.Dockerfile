ARG RUST_BUILDER_IMAGE
FROM node:24-bookworm-slim AS node

FROM ${RUST_BUILDER_IMAGE}

RUN apt-get update && \
    apt-get install -y --no-install-recommends btrfs-progs gpg squashfs-tools && \
    rm -rf /var/lib/apt/lists/*

COPY --from=node /usr/local/bin/node /usr/local/bin/node
