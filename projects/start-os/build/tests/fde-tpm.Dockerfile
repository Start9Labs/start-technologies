FROM debian:trixie-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends cryptsetup-bin tpm2-tools tpm2-abrmd dbus swtpm libtss2-tcti-swtpm0 libtss2-tcti-tabrmd0 \
    && rm -rf /var/lib/apt/lists/*
