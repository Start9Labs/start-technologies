#!/bin/bash
set -e


echo "==== StartOS Image Build ===="

echo "Building for architecture: $IB_TARGET_ARCH"

SOURCE_DIR="$(realpath $(dirname "${BASH_SOURCE[0]}"))"

base_dir="$(pwd	-P)"
prep_results_dir="$base_dir/images-prep"
RESULTS_DIR="$base_dir/results"
echo "Saving results in: $RESULTS_DIR"

DEB_PATH="$base_dir/$1"

VERSION="$(dpkg-deb --fsys-tarfile $DEB_PATH | tar --to-stdout -xvf - ./usr/lib/startos/VERSION.txt)"
GIT_HASH="$(dpkg-deb --fsys-tarfile $DEB_PATH | tar --to-stdout -xvf - ./usr/lib/startos/GIT_HASH.txt)"
if [[ "$GIT_HASH" =~ ^@ ]]; then
  GIT_HASH="unknown"
else
  GIT_HASH="$(echo -n "$GIT_HASH" |  head -c 7)"
fi
IB_OS_ENV="$(dpkg-deb --fsys-tarfile $DEB_PATH | tar --to-stdout -xvf - ./usr/lib/startos/ENVIRONMENT.txt)"
IB_TARGET_PLATFORM="$(dpkg-deb --fsys-tarfile $DEB_PATH | tar --to-stdout -xvf - ./usr/lib/startos/PLATFORM.txt)"

VERSION_FULL="${VERSION}-${GIT_HASH}"
if [ -n "$IB_OS_ENV" ]; then
  VERSION_FULL="$VERSION_FULL~${IB_OS_ENV}"
fi

IMAGE_BASENAME=startos-${VERSION_FULL}_${IB_TARGET_PLATFORM}

BOOTLOADERS=grub-efi
if [ "$IB_TARGET_PLATFORM" = "x86_64" ] || [ "$IB_TARGET_PLATFORM" = "x86_64-nonfree" ] || [ "$IB_TARGET_PLATFORM" = "x86_64-nvidia" ]; then
	IB_TARGET_ARCH=amd64
	QEMU_ARCH=x86_64
	BOOTLOADERS=grub-efi,syslinux
elif [ "$IB_TARGET_PLATFORM" = "aarch64" ] || [ "$IB_TARGET_PLATFORM" = "aarch64-nonfree" ] || [ "$IB_TARGET_PLATFORM" = "aarch64-nvidia" ] || [ "$IB_TARGET_PLATFORM" = "raspberrypi" ]  || [ "$IB_TARGET_PLATFORM" = "rockchip64" ]; then
	IB_TARGET_ARCH=arm64
	QEMU_ARCH=aarch64
elif [ "$IB_TARGET_PLATFORM" = "riscv64" ] || [ "$IB_TARGET_PLATFORM" = "riscv64-nonfree" ]; then
	IB_TARGET_ARCH=riscv64
	QEMU_ARCH=riscv64
else
	IB_TARGET_ARCH="$IB_TARGET_PLATFORM"
	QEMU_ARCH="$IB_TARGET_PLATFORM"
fi

QEMU_ARGS=()
if [ "$QEMU_ARCH" != $(uname -m) ]; then
	QEMU_ARGS+=(--bootstrap-qemu-arch ${IB_TARGET_ARCH})
	QEMU_ARGS+=(--bootstrap-qemu-static /usr/bin/qemu-${QEMU_ARCH}-static)
fi

mkdir -p $prep_results_dir

cd $prep_results_dir

NON_FREE=
if [[ "${IB_TARGET_PLATFORM}" =~ -nonfree$ ]] || [[ "${IB_TARGET_PLATFORM}" =~ -nvidia$ ]] || [ "${IB_TARGET_PLATFORM}" = "raspberrypi" ]; then
	NON_FREE=1
fi
NVIDIA=
if [[ "${IB_TARGET_PLATFORM}" =~ -nvidia$ ]]; then
	NVIDIA=1
fi
IMAGE_TYPE=iso
if [ "${IB_TARGET_PLATFORM}" = "raspberrypi" ] || [ "${IB_TARGET_PLATFORM}" = "rockchip64" ]; then
	IMAGE_TYPE=img
fi

ARCHIVE_AREAS="main contrib"
if [ "$NON_FREE" = 1 ]; then
	if [ "$IB_SUITE" = "bullseye" ]; then
		ARCHIVE_AREAS="$ARCHIVE_AREAS non-free"
	else
		ARCHIVE_AREAS="$ARCHIVE_AREAS non-free non-free-firmware"
	fi
fi

PLATFORM_CONFIG_EXTRAS=()
if [ "${IB_TARGET_PLATFORM}" = "raspberrypi" ]; then
	PLATFORM_CONFIG_EXTRAS+=( --firmware-binary false )
	PLATFORM_CONFIG_EXTRAS+=( --firmware-chroot false )
	RPI_KERNEL_VERSION=6.18.33+rpt
	PLATFORM_CONFIG_EXTRAS+=( --linux-packages linux-image-$RPI_KERNEL_VERSION )
	PLATFORM_CONFIG_EXTRAS+=( --linux-flavours "rpi-v8 rpi-2712" )
elif [ "${IB_TARGET_PLATFORM}" = "rockchip64" ]; then
	PLATFORM_CONFIG_EXTRAS+=( --linux-flavours rockchip64 )
elif [ "${IB_TARGET_ARCH}" = "riscv64" ]; then
	PLATFORM_CONFIG_EXTRAS+=( --uefi-secure-boot=disable )
fi


cat > /etc/wgetrc << EOF
retry_connrefused = on
tries = 100
EOF
lb config \
	--iso-application "StartOS v${VERSION_FULL} ${IB_TARGET_ARCH}" \
	--iso-volume "StartOS v${VERSION} ${IB_TARGET_ARCH}" \
	--iso-preparer "START9 LABS; HTTPS://START9.COM" \
	--iso-publisher "START9 LABS; HTTPS://START9.COM" \
	--backports true \
	--bootappend-live "boot=live noautologin console=tty0" \
	--bootloaders $BOOTLOADERS \
	--cache false \
	--mirror-bootstrap "https://deb.debian.org/debian/" \
	--mirror-chroot "https://deb.debian.org/debian/" \
	--mirror-chroot-security "https://security.debian.org/debian-security" \
	-d ${IB_SUITE} \
	-a ${IB_TARGET_ARCH} \
	${QEMU_ARGS[@]} \
	--archive-areas "${ARCHIVE_AREAS}" \
	"${PLATFORM_CONFIG_EXTRAS[@]}"

# Overlays

mkdir -p config/packages.chroot/
cp $RESULTS_DIR/$IMAGE_BASENAME.deb config/packages.chroot/
dpkg-name config/packages.chroot/*.deb

mkdir -p config/includes.chroot/etc
echo start > config/includes.chroot/etc/hostname
cat > config/includes.chroot/etc/hosts << EOT
127.0.0.1       localhost start
::1             localhost start ip6-localhost ip6-loopback
ff02::1         ip6-allnodes
ff02::2         ip6-allrouters
EOT

if [[ "${IB_OS_ENV}" =~ (^|-)dev($|-) ]]; then
	mkdir -p config/includes.chroot/etc/ssh/sshd_config.d
	echo "PasswordAuthentication yes" > config/includes.chroot/etc/ssh/sshd_config.d/dev-password-auth.conf
fi

# Installer marker file (used by installed GRUB to detect the live USB)
mkdir -p config/includes.binary
touch config/includes.binary/.startos-installer

if [ "${IB_TARGET_PLATFORM}" = "raspberrypi" ]; then
	mkdir -p config/includes.chroot
	git clone --depth=1 --branch=stable https://github.com/raspberrypi/rpi-firmware.git config/includes.chroot/boot
	rm -rf config/includes.chroot/boot/.git config/includes.chroot/boot/modules
	rsync -rLp $SOURCE_DIR/raspberrypi/squashfs/ config/includes.chroot/
fi

# Bootloaders

rm -rf config/bootloaders
cp -r /usr/share/live/build/bootloaders config/bootloaders

cat > config/bootloaders/syslinux/syslinux.cfg << EOF
include menu.cfg
default vesamenu.c32
prompt 0
timeout 50
EOF

cat > config/bootloaders/isolinux/isolinux.cfg << EOF
include menu.cfg
default vesamenu.c32
prompt 0
timeout 50
EOF

# Extract splash.png from the deb package
dpkg-deb --fsys-tarfile $DEB_PATH | tar --to-stdout -xf - ./usr/lib/startos/splash.png > /tmp/splash.png
cp /tmp/splash.png config/bootloaders/syslinux_common/splash.png
cp /tmp/splash.png config/bootloaders/isolinux/splash.png
cp /tmp/splash.png config/bootloaders/grub-pc/splash.png
rm /tmp/splash.png

sed -i -e '2i set timeout=5' config/bootloaders/grub-pc/config.cfg

# Archives

mkdir -p config/archives

if [ "${IB_TARGET_PLATFORM}" = "raspberrypi" ]; then
	# Fetch the keyring package (not the old raspberrypi.gpg.key, which has
	# SHA1-only binding signatures that sqv on Trixie rejects).
	KEYRING_DEB=$(mktemp)
	curl -fsSL -o "$KEYRING_DEB" https://archive.raspberrypi.com/debian/pool/main/r/raspberrypi-archive-keyring/raspberrypi-archive-keyring_2025.1+rpt1_all.deb
	dpkg-deb -x "$KEYRING_DEB" "$KEYRING_DEB.d"
	cp "$KEYRING_DEB.d/usr/share/keyrings/raspberrypi-archive-keyring.gpg" config/archives/raspi.key
	rm -rf "$KEYRING_DEB" "$KEYRING_DEB.d"
	echo "deb [arch=${IB_TARGET_ARCH} signed-by=/etc/apt/trusted.gpg.d/raspi.key.gpg] https://archive.raspberrypi.com/debian/ ${IB_SUITE} main" > config/archives/raspi.list
fi

if [ "${IB_TARGET_PLATFORM}" = "rockchip64" ]; then
	curl -fsSL https://apt.armbian.com/armbian.key | gpg --dearmor -o config/archives/armbian.key
	echo "deb https://apt.armbian.com/ ${IB_SUITE} main" > config/archives/armbian.list
fi

if [ "$NVIDIA" = 1 ]; then
	curl -fsSL https://nvidia.github.io/libnvidia-container/gpgkey | gpg --dearmor -o config/archives/nvidia-container-toolkit.key
	curl -s -L https://nvidia.github.io/libnvidia-container/stable/deb/nvidia-container-toolkit.list \
		| sed 's#deb https://#deb [signed-by=/etc/apt/trusted.gpg.d/nvidia-container-toolkit.key.gpg] https://#g' \
		> config/archives/nvidia-container-toolkit.list
fi

if [ "${IB_TARGET_PLATFORM}" = "raspberrypi" ]; then
# Pin the Pi to exactly the versioned +rpt kernels live-build installs. Two
# things would otherwise outrank them in GRUB's version-sorted default and boot
# instead: Debian's generic/RT kernel (a newer version), and the unversioned RPT
# kernel metas (linux-image-rpi-*) which track the latest +rpt release and get
# pulled transitively. Forbid both so /boot holds only the pinned vendor kernels.
cat > config/archives/backports.pref <<-EOF
Package: *nvidia*
Pin: release n=${IB_SUITE}-backports
Pin-Priority: 500

Package: linux-image-* linux-headers-*
Pin: origin "deb.debian.org"
Pin-Priority: -1

Package: linux-image-* linux-headers-*
Pin: origin "security.debian.org"
Pin-Priority: -1

Package: linux-image-rpi-* linux-headers-rpi-*
Pin: version *
Pin-Priority: -1
EOF
else
cat > config/archives/backports.pref <<-EOF
Package: linux-image-*
Pin: release n=${IB_SUITE}-backports
Pin-Priority: 500

Package: linux-headers-*
Pin: release n=${IB_SUITE}-backports
Pin-Priority: 500

Package: *nvidia*
Pin: release n=${IB_SUITE}-backports
Pin-Priority: 500
EOF
fi

# Hooks

cat > config/hooks/normal/9000-install-startos.hook.chroot << EOF
#!/bin/bash

set -e

if [ "${IB_TARGET_PLATFORM}" != "raspberrypi" ]; then
    /usr/lib/startos/scripts/enable-kiosk
fi

if [ "${NVIDIA}" = "1" ]; then
    # install a specific NVIDIA driver version

    # ---------------- configuration ----------------
    NVIDIA_DRIVER_VERSION="\${NVIDIA_DRIVER_VERSION:-580.173.02}"

    BASE_URL="https://download.nvidia.com/XFree86/Linux-${QEMU_ARCH}"

    echo "[nvidia-hook] Using NVIDIA driver: \${NVIDIA_DRIVER_VERSION}" >&2

    # ---------------- kernel version ----------------

    # Determine target kernel version from newest /boot/vmlinuz-* in the chroot.
    KVER="\$(
        ls -1t /boot/vmlinuz-* 2>/dev/null \
            | head -n1 \
            | sed 's|.*/vmlinuz-||'
    )"

    if [ -z "\${KVER}" ]; then
        echo "[nvidia-hook] ERROR: no /boot/vmlinuz-* found; cannot determine kernel version" >&2
        exit 1
    fi

    echo "[nvidia-hook] Target kernel version: \${KVER}" >&2

    # Ensure kernel headers are present
	TEMP_APT_DEPS=(build-essential pkg-config)
    if [ ! -e "/lib/modules/\${KVER}/build" ]; then
		TEMP_APT_DEPS+=(linux-headers-\${KVER})
    fi

    echo "[nvidia-hook] Installing build dependencies" >&2

	/usr/lib/startos/scripts/install-equivs <<-EOF
	Package: nvidia-depends
	Version: \${NVIDIA_DRIVER_VERSION}
	Section: unknown
	Priority: optional
	Depends: \${dep_list="\$(IFS=', '; echo "\${TEMP_APT_DEPS[*]}")"}
	EOF

    # ---------------- download and run installer ----------------

    RUN_NAME="NVIDIA-Linux-${QEMU_ARCH}-\${NVIDIA_DRIVER_VERSION}.run"
    RUN_PATH="/root/\${RUN_NAME}"
    RUN_URL="\${BASE_URL}/\${NVIDIA_DRIVER_VERSION}/\${RUN_NAME}"

    echo "[nvidia-hook] Downloading \${RUN_URL}" >&2
    wget -O "\${RUN_PATH}" "\${RUN_URL}"
    chmod +x "\${RUN_PATH}"

    echo "[nvidia-hook] Running NVIDIA installer for kernel \${KVER}" >&2

    # Otherwise the installer picks the flavour from the build host's own GPUs.
    if ! sh "\${RUN_PATH}" \
        --silent \
        --kernel-module-type=proprietary \
        --kernel-name="\${KVER}" \
        --no-x-check \
        --no-nouveau-check \
        --no-runlevel-check; then
		cat /var/log/nvidia-installer.log
		exit 1
	fi

    # Rebuild module metadata
    echo "[nvidia-hook] Running depmod for \${KVER}" >&2
    depmod -a "\${KVER}"

    echo "[nvidia-hook] NVIDIA \${NVIDIA_DRIVER_VERSION} installation complete for kernel \${KVER}" >&2

    echo "[nvidia-hook] Removing .run installer..." >&2
    rm -f "\${RUN_PATH}"

    echo "[nvidia-hook] Blacklisting nouveau..." >&2
    echo "blacklist nouveau" > /etc/modprobe.d/blacklist-nouveau.conf
    echo "options nouveau modeset=0" >> /etc/modprobe.d/blacklist-nouveau.conf

    echo "[nvidia-hook] Rebuilding initramfs..." >&2
    update-initramfs -u -k "\${KVER}"

    echo "[nvidia-hook] Removing build dependencies..." >&2
	apt-get purge -y nvidia-depends
	apt-get autoremove -y
    echo "[nvidia-hook] Removed build dependencies." >&2
fi

# Install linux-kbuild for sign-file (Secure Boot module signing)
KVER_ALL="\$(ls -1t /boot/vmlinuz-* 2>/dev/null | head -n1 | sed 's|.*/vmlinuz-||')"
if [ -n "\${KVER_ALL}" ]; then
    KBUILD_VER="\$(echo "\${KVER_ALL}" | grep -oP '^\d+\.\d+')"
    if [ -n "\${KBUILD_VER}" ]; then
        echo "[build] Installing linux-kbuild-\${KBUILD_VER} for Secure Boot support" >&2
        apt-get install -y "linux-kbuild-\${KBUILD_VER}" || echo "[build] WARNING: linux-kbuild-\${KBUILD_VER} not available" >&2
    fi
fi

cp /etc/resolv.conf /etc/resolv.conf.bak

if [ "${IB_SUITE}" = trixie ] && [ "${IB_TARGET_ARCH}" != riscv64 ]; then
    echo 'deb https://deb.debian.org/debian/ bookworm main' > /etc/apt/sources.list.d/bookworm.list
    apt-get update
    apt-get install -y postgresql-15
    rm /etc/apt/sources.list.d/bookworm.list
    apt-get update
    systemctl mask postgresql
fi

if [ "${IB_TARGET_PLATFORM}" = "raspberrypi" ]; then
    ln -sf /usr/bin/pi-beep /usr/local/bin/beep
    mkinitramfs -c gzip -o /boot/initrd.img-${RPI_KERNEL_VERSION}-rpi-v8 ${RPI_KERNEL_VERSION}-rpi-v8
    mkinitramfs -c gzip -o /boot/initrd.img-${RPI_KERNEL_VERSION}-rpi-2712 ${RPI_KERNEL_VERSION}-rpi-2712
    cp /usr/lib/u-boot/rpi_arm64/u-boot.bin /boot/firmware/u-boot.bin
fi

useradd --shell /bin/bash -G startos -m start9
echo start9:embassy | chpasswd
usermod -aG sudo start9
usermod -aG systemd-journal start9

echo "start9 ALL=(ALL:ALL) NOPASSWD: ALL" | sudo tee "/etc/sudoers.d/010_start9-nopasswd"

if ! [[ "${IB_OS_ENV}" =~ (^|-)dev($|-) ]]; then
    passwd -l start9
fi

mkdir -p /media/startos
chmod 750 /media/startos
chown root:startos /media/startos

start-cli --registry=https://alpha-registry-x.start9.com registry package download tor -d /usr/lib/startos/tor_${QEMU_ARCH}.s9pk -a "${QEMU_ARCH}"

EOF

SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-$(date '+%s')}"

if lb bootstrap; then
	true
else
	EXIT=$?
	cat ./chroot/debootstrap/debootstrap.log
	exit $EXIT
fi
lb chroot
lb installer
lb binary_chroot
lb chroot_prep install all mode-apt-install-binary mode-archives-chroot
mv chroot/chroot/etc/resolv.conf.bak chroot/chroot/etc/resolv.conf
lb binary_rootfs

cp $prep_results_dir/binary/live/filesystem.squashfs $RESULTS_DIR/$IMAGE_BASENAME.squashfs

if [ "${IMAGE_TYPE}" = iso ]; then

	lb binary_manifest
	lb binary_package-lists
	lb binary_linux-image
	lb binary_memtest
	lb binary_grub-legacy
	lb binary_grub-pc
	lb binary_grub_cfg
	lb binary_syslinux
	lb binary_disk
	lb binary_loadlin
	lb binary_win32-loader
	lb binary_includes
	lb binary_grub-efi
	lb binary_hooks
	lb binary_checksums
	find binary -newermt "$(date -d@${SOURCE_DATE_EPOCH} '+%Y-%m-%d %H:%M:%S')" -printf "%y %p\n" -exec touch '{}' -d@${SOURCE_DATE_EPOCH} --no-dereference ';' > binary.modified_timestamps
	lb binary_iso
	lb binary_onie
	lb binary_netboot
	lb binary_tar
	lb binary_hdd
	lb binary_zsync
	lb chroot_prep remove all mode-archives-chroot
	lb source

	mv $prep_results_dir/live-image-${IB_TARGET_ARCH}.hybrid.iso $RESULTS_DIR/$IMAGE_BASENAME.iso

elif [ "${IMAGE_TYPE}" = img ]; then

	SECTOR_LEN=512
	FW_START=$((1024 * 1024)) # 1MiB (sector 2048) — Pi-specific
	FW_LEN=$((128 * 1024 * 1024)) # 128MiB (Pi firmware + U-Boot + DTBs)
	FW_END=$((FW_START + FW_LEN - 1))
	ESP_START=$((FW_END + 1)) # 100MB EFI System Partition (matches os_install)
	ESP_LEN=$((100 * 1024 * 1024))
	ESP_END=$((ESP_START + ESP_LEN - 1))
	BOOT_START=$((ESP_END + 1)) # 2GB /boot (matches os_install)
	BOOT_LEN=$((2 * 1024 * 1024 * 1024))
	BOOT_END=$((BOOT_START + BOOT_LEN - 1))
	ROOT_START=$((BOOT_END + 1))

	# Size root partition to fit the squashfs + 256MB overhead for btrfs
	# metadata and config overlay, avoiding the need for btrfs resize
	SQUASHFS_SIZE=$(stat -c %s $prep_results_dir/binary/live/filesystem.squashfs)
	ROOT_LEN=$(( SQUASHFS_SIZE + 256 * 1024 * 1024 ))
	# Align to sector boundary
	ROOT_LEN=$(( (ROOT_LEN + SECTOR_LEN - 1) / SECTOR_LEN * SECTOR_LEN ))

	# Total image: partitions + GPT backup header (34 sectors)
	IMG_LEN=$((ROOT_START + ROOT_LEN + 34 * SECTOR_LEN))

	# Fixed GPT partition UUIDs (deterministic, based on old MBR disk ID cb15ae4d)
	FW_UUID=cb15ae4d-0001-4000-8000-000000000001
	ESP_UUID=cb15ae4d-0002-4000-8000-000000000002
	BOOT_UUID=cb15ae4d-0003-4000-8000-000000000003
	ROOT_UUID=cb15ae4d-0004-4000-8000-000000000004

	TARGET_NAME=$prep_results_dir/${IMAGE_BASENAME}.img
	truncate -s $IMG_LEN $TARGET_NAME

	sfdisk $TARGET_NAME <<-EOF
		label: gpt

		${TARGET_NAME}1 : start=$((FW_START / SECTOR_LEN)), size=$((FW_LEN / SECTOR_LEN)), type=EBD0A0A2-B9E5-4433-87C0-68B6B72699C7, uuid=${FW_UUID}, name="firmware"
		${TARGET_NAME}2 : start=$((ESP_START / SECTOR_LEN)), size=$((ESP_LEN / SECTOR_LEN)), type=C12A7328-F81F-11D2-BA4B-00A0C93EC93B, uuid=${ESP_UUID}, name="efi"
		${TARGET_NAME}3 : start=$((BOOT_START / SECTOR_LEN)), size=$((BOOT_LEN / SECTOR_LEN)), type=0FC63DAF-8483-4772-8E79-3D69D8477DE4, uuid=${BOOT_UUID}, name="boot"
		${TARGET_NAME}4 : start=$((ROOT_START / SECTOR_LEN)), size=$((ROOT_LEN / SECTOR_LEN)), type=B921B045-1DF0-41C3-AF44-4C6F280D3FAE, uuid=${ROOT_UUID}, name="root"
	EOF

	# Create named loop device nodes (high minor numbers to avoid conflicts)
	# and detach any stale ones from previous failed builds
	FW_DEV=/dev/startos-loop-fw
	ESP_DEV=/dev/startos-loop-esp
	BOOT_DEV=/dev/startos-loop-boot
	ROOT_DEV=/dev/startos-loop-root
	for dev in $FW_DEV:200 $ESP_DEV:201 $BOOT_DEV:202 $ROOT_DEV:203; do
		name=${dev%:*}
		minor=${dev#*:}
		[ -e $name ] || mknod $name b 7 $minor
		losetup -d $name 2>/dev/null || true
	done

	losetup $FW_DEV --offset $FW_START --sizelimit $FW_LEN $TARGET_NAME
	losetup $ESP_DEV --offset $ESP_START --sizelimit $ESP_LEN $TARGET_NAME
	losetup $BOOT_DEV --offset $BOOT_START --sizelimit $BOOT_LEN $TARGET_NAME
	losetup $ROOT_DEV --offset $ROOT_START --sizelimit $ROOT_LEN $TARGET_NAME

	mkfs.vfat -F32 -n firmware $FW_DEV
	mkfs.vfat -F32 -n efi $ESP_DEV
	mkfs.vfat -F32 -n boot $BOOT_DEV
	mkfs.btrfs -f -L rootfs $ROOT_DEV

	TMPDIR=$(mktemp -d)

	# Extract boot files from squashfs to staging area
	BOOT_STAGING=$(mktemp -d)
	unsquashfs -n -f -d $BOOT_STAGING $prep_results_dir/binary/live/filesystem.squashfs boot

	# Mount partitions (nested: firmware and efi inside boot)
	mkdir -p $TMPDIR/boot $TMPDIR/root
	mount $BOOT_DEV $TMPDIR/boot
	mkdir -p $TMPDIR/boot/firmware $TMPDIR/boot/efi
	mount $FW_DEV $TMPDIR/boot/firmware
	mount $ESP_DEV $TMPDIR/boot/efi
	mount $ROOT_DEV $TMPDIR/root

	# Copy boot files — nested mounts route firmware/* to the firmware partition
	cp -a $BOOT_STAGING/boot/. $TMPDIR/boot/
	rm -rf $BOOT_STAGING

	mkdir $TMPDIR/root/images $TMPDIR/root/config

	# A pre-installed image == post-os_install state, so it needs setup.json (the
	# SetupInfo marker) or core boots the install wizard instead of setup. It's
	# written at first boot by init_resize, which knows the runtime OS drive path.

	B3SUM=$(b3sum $prep_results_dir/binary/live/filesystem.squashfs | head -c 16)
	cp $prep_results_dir/binary/live/filesystem.squashfs $TMPDIR/root/images/$B3SUM.rootfs
	ln -rsf $TMPDIR/root/images/$B3SUM.rootfs $TMPDIR/root/config/current.rootfs

	mkdir -p $TMPDIR/next $TMPDIR/lower $TMPDIR/root/config/work $TMPDIR/root/config/overlay
	mount $TMPDIR/root/config/current.rootfs $TMPDIR/lower

	mount -t overlay -o lowerdir=$TMPDIR/lower,workdir=$TMPDIR/root/config/work,upperdir=$TMPDIR/root/config/overlay overlay $TMPDIR/next

	if [ "${IB_TARGET_PLATFORM}" = "raspberrypi" ]; then
		rsync -a $SOURCE_DIR/raspberrypi/img/ $TMPDIR/next/

		# Install GRUB: ESP at /boot/efi (Part 2), /boot (Part 3)
		mkdir -p $TMPDIR/next/boot \
			$TMPDIR/next/dev $TMPDIR/next/proc $TMPDIR/next/sys $TMPDIR/next/media/startos/root
		mount --rbind $TMPDIR/boot $TMPDIR/next/boot
		mount --bind /dev $TMPDIR/next/dev
		mount -t proc proc $TMPDIR/next/proc
		mount -t sysfs sysfs $TMPDIR/next/sys
		mount --bind $TMPDIR/root $TMPDIR/next/media/startos/root

		chroot $TMPDIR/next grub-install --target=arm64-efi --removable --efi-directory=/boot/efi --boot-directory=/boot --no-nvram
		chroot $TMPDIR/next update-grub

		umount $TMPDIR/next/media/startos/root
		umount $TMPDIR/next/sys
		umount $TMPDIR/next/proc
		umount $TMPDIR/next/dev
		umount -l $TMPDIR/next/boot

		# Fix root= in grub.cfg: update-grub sees loop devices, but the
		# real device uses a fixed GPT PARTUUID for root (Part 4).
		sed -i "s|root=[^ ]*|root=PARTUUID=${ROOT_UUID}|g" $TMPDIR/boot/grub/grub.cfg

		# Inject first-boot resize script into GRUB config
		sed -i 's| boot=startos| boot=startos init=/usr/lib/startos/scripts/init_resize\.sh|' $TMPDIR/boot/grub/grub.cfg
	fi

	umount $TMPDIR/next
	umount $TMPDIR/lower

	umount $TMPDIR/boot/firmware
	umount $TMPDIR/boot/efi
	umount $TMPDIR/boot
	umount $TMPDIR/root

	losetup -d $ROOT_DEV
	losetup -d $BOOT_DEV
	losetup -d $ESP_DEV
	losetup -d $FW_DEV

	mv $TARGET_NAME $RESULTS_DIR/$IMAGE_BASENAME.img

fi

chown $IB_UID:$IB_UID $RESULTS_DIR/$IMAGE_BASENAME.*
