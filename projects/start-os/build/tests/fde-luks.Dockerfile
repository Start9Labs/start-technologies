FROM debian:trixie-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends cryptsetup-bin \
    && rm -rf /var/lib/apt/lists/*
