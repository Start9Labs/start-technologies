FROM debian:trixie-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends cryptsetup-bin passwd \
    && useradd --no-create-home start9 \
    && useradd --no-create-home kiosk \
    && rm -rf /var/lib/apt/lists/*
