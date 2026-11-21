FROM debian:trixie-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends cryptsetup-bin udev tpm2-tools tpm2-abrmd dbus swtpm libtss2-tcti-swtpm0 libtss2-tcti-tabrmd0 \
    && rm -rf /var/lib/apt/lists/*
COPY --chmod=755 fde-tpm-run /usr/local/bin/fde-tpm-run
