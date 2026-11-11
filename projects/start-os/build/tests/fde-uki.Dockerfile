FROM debian:trixie-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends systemd-ukify systemd-boot-efi systemd sbsigntool binutils openssl jq \
    && rm -rf /var/lib/apt/lists/*
