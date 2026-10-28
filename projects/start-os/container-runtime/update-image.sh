#!/bin/bash

cd "$(dirname "${BASH_SOURCE[0]}")"

set -e

RUST_ARCH="$ARCH"
if [ "$ARCH" = "riscv64" ]; then
  RUST_ARCH="riscv64gc"
fi

mount -t tmpfs tmpfs /tmp
mkdir -p /tmp/lower /tmp/upper /tmp/work /tmp/combined
mount -o loop debian.${ARCH}.squashfs /tmp/lower
mount -t overlay -olowerdir=/tmp/lower,upperdir=/tmp/upper,workdir=/tmp/work overlay /tmp/combined

mkdir -p /tmp/combined/usr/lib/startos/
cp ../build/lib/resolved.conf /tmp/combined/usr/lib/startos/resolved.conf
rsync -a --copy-unsafe-links --info=progress2 dist/ /tmp/combined/usr/lib/startos/init/
chown -R 0:0 /tmp/combined/usr/lib/startos/
cp container-runtime.service /tmp/combined/lib/systemd/system/container-runtime.service
chown 0:0 /tmp/combined/lib/systemd/system/container-runtime.service
cp container-runtime-failure.service /tmp/combined/lib/systemd/system/container-runtime-failure.service
chown 0:0 /tmp/combined/lib/systemd/system/container-runtime-failure.service
cp ../../../target/${RUST_ARCH}-unknown-linux-musl/release/start-container /tmp/combined/usr/bin/start-container
echo -e '#!/bin/bash\nexec start-container "$@"' > /tmp/combined/usr/bin/start-cli # TODO: remove
chmod +x /tmp/combined/usr/bin/start-cli
chown 0:0 /tmp/combined/usr/bin/start-container
echo container-runtime | sha256sum | head -c 32 | cat - <(echo) > /tmp/combined/etc/machine-id
rm -f /tmp/combined/etc/resolv.conf
cp /etc/resolv.conf /tmp/combined/etc/resolv.conf
for fs in proc sys dev; do
    mount --bind /$fs /tmp/combined/$fs
done
cat deb-install.sh | chroot /tmp/combined /bin/bash
for fs in proc sys dev; do
    umount /tmp/combined/$fs
done
truncate -s 0 /tmp/combined/etc/machine-id

rm -f rootfs.${ARCH}.squashfs
mkdir -p ../build/lib/container-runtime
mksquashfs /tmp/combined rootfs.${ARCH}.squashfs