FROM debian:trixie-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends cryptsetup-bin passwd openssh-server openssl systemd-ukify python3-cryptography \
    && groupadd --system startos \
    && useradd --no-create-home start9 \
    && useradd --no-create-home kiosk \
    && rm -rf /var/lib/apt/lists/*
