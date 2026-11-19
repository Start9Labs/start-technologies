ARG SUITE=trixie

FROM debian:${SUITE}

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && \
    apt-get install -yq \
    live-build \
    procps \
    binfmt-support \
    binutils \
    qemu-utils \
    qemu-user-static \
    xorriso \
    isolinux \
    ca-certificates \
    curl \
    wget \
    gpg \
    git \
    fdisk \
    dosfstools \
    e2fsprogs \
    squashfs-tools \
    rsync \
    b3sum \
    btrfs-progs \
    gdisk \
    dpkg-dev


COPY binary_grub-efi.patch /root/binary_grub-efi.patch
RUN patch /usr/lib/live/build/binary_grub-efi < /root/binary_grub-efi.patch && rm /root/binary_grub-efi.patch

RUN echo 'retry_connrefused = on' > /etc/wgetrc && \
    echo 'tries = 100' >> /etc/wgetrc

WORKDIR /root