FROM debian:trixie-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends network-manager dbus \
    && rm -rf /var/lib/apt/lists/*
