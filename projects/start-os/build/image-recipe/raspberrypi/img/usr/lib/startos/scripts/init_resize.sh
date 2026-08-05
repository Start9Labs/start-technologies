#!/bin/bash
# Derived from RPi-Distro/raspi-config usr/lib/raspi-config/init_resize.sh,
# MIT licensed, Copyright 2012 Alex Bradbury.

log() { echo "init_resize: $*"; }

get_variables () {
  ROOT_PART_DEV=$(findmnt /media/startos/root -o source -n)
  ROOT_PART_NAME=$(echo "$ROOT_PART_DEV" | cut -d "/" -f 3)
  ROOT_DEV_NAME=$(echo /sys/block/*/"${ROOT_PART_NAME}" | cut -d "/" -f 4)
  ROOT_DEV="/dev/${ROOT_DEV_NAME}"
  ROOT_PART_NUM=$(cat "/sys/block/${ROOT_DEV_NAME}/${ROOT_PART_NAME}/partition")

  # Relocate the GPT backup header to the disk end before any parted call; on a
  # freshly-flashed (larger) card it sits mid-disk and parted otherwise blocks
  # on an interactive Fix/Ignore prompt, freezing the headless boot.
  sgdisk -e "$ROOT_DEV" 2>/dev/null || true

  BOOT_PART_DEV=$(findmnt /boot -o source -n)
  BOOT_PART_NAME=$(echo "$BOOT_PART_DEV" | cut -d "/" -f 3)
  BOOT_DEV_NAME=$(echo /sys/block/*/"${BOOT_PART_NAME}" | cut -d "/" -f 4)
  BOOT_PART_NUM=$(cat "/sys/block/${BOOT_DEV_NAME}/${BOOT_PART_NAME}/partition")

  ROOT_DEV_SIZE=$(cat "/sys/block/${ROOT_DEV_NAME}/size")
  # GPT backup header/entries occupy last 33 sectors
  USABLE_END=$((ROOT_DEV_SIZE - 34))

  if [ "$USABLE_END" -le 67108864 ]; then
      TARGET_END=$USABLE_END
  else
      TARGET_END=$((33554432 - 1))
      DATA_PART_START=33554432
      DATA_PART_END=$USABLE_END
  fi

  PARTITION_TABLE=$(parted -ms "$ROOT_DEV" unit s print | tr -d 's')

  LAST_PART_NUM=$(echo "$PARTITION_TABLE" | tail -n 1 | cut -d ":" -f 1)

  ROOT_PART_LINE=$(echo "$PARTITION_TABLE" | grep -e "^${ROOT_PART_NUM}:")
  ROOT_PART_START=$(echo "$ROOT_PART_LINE" | cut -d ":" -f 2)
  ROOT_PART_END=$(echo "$ROOT_PART_LINE" | cut -d ":" -f 3)
}

check_variables () {
  if [ "$BOOT_DEV_NAME" != "$ROOT_DEV_NAME" ]; then
      FAIL_REASON="Boot and root partitions are on different devices"
      return 1
  fi

  if [ "$ROOT_PART_NUM" -ne "$LAST_PART_NUM" ]; then
    FAIL_REASON="Root partition should be last partition"
    return 1
  fi

  if [ "$ROOT_PART_END" -gt "$TARGET_END" ]; then
    FAIL_REASON="Root partition runs past the end of device"
    return 1
  fi

  if [ ! -b "$ROOT_DEV" ] || [ ! -b "$ROOT_PART_DEV" ] || [ ! -b "$BOOT_PART_DEV" ] ; then
    FAIL_REASON="Could not determine partitions"
    return 1
  fi
}

main () {
  log "reading partition layout"
  get_variables
  log "root=$ROOT_PART_DEV dev=$ROOT_DEV part=$ROOT_PART_NUM last=$LAST_PART_NUM end=$ROOT_PART_END target=$TARGET_END"

  if ! check_variables; then
    return 1
  fi

  # Root is mounted, so parted's interactive "partition is in use, continue?"
  # prompt can't be answered reliably (this parted parses a piped answer as a
  # command -> "invalid token"). Use sfdisk --no-reread to rewrite the table
  # (GPT backup already relocated to the disk end by sgdisk -e in get_variables),
  # then partx -u to update the mounted partitions' sizes live via BLKPG, so
  # btrfs can grow without a reboot.
  if [ -n "$DATA_PART_START" ]; then
    log "resizing root $ROOT_PART_NUM to ${TARGET_END}s + appending data partition"
    if ! echo ", $((TARGET_END - ROOT_PART_START + 1))" | sfdisk --no-reread -N "$ROOT_PART_NUM" "$ROOT_DEV"; then
      FAIL_REASON="Root partition resize failed"
      return 1
    fi
    if ! echo "${DATA_PART_START}, +" | sfdisk --no-reread --append "$ROOT_DEV"; then
      FAIL_REASON="Data partition creation failed"
      return 1
    fi
  else
    log "growing root $ROOT_PART_NUM to fill the disk"
    if ! echo ", +" | sfdisk --no-reread -N "$ROOT_PART_NUM" "$ROOT_DEV"; then
      FAIL_REASON="Root partition resize failed"
      return 1
    fi
  fi

  partx -u "$ROOT_DEV" || true

  log "remounting root rw and growing btrfs"
  mount / -o remount,rw
  btrfs filesystem resize max /media/startos/root

  log "generating machine-id"
  if ! systemd-machine-id-setup --root=/media/startos/config/overlay/; then
    FAIL_REASON="systemd-machine-id-setup failed"
    return 1
  fi

  log "generating ssh host keys"
  if ! (mkdir -p /media/startos/config/overlay/etc/ssh && ssh-keygen -A -f /media/startos/config/overlay/); then
    FAIL_REASON="ssh host key generation failed"
    return 1
  fi

  # Write the pre-installed SetupInfo marker. Without setup.json core boots the
  # install wizard instead of setup; with it and a null guid (the data drive is
  # created during the user's setup) the wizard asks only for a data drive.
  # osDrive is the OS drive, only knowable at runtime (mmcblk0 for SD, sda for
  # USB boot), which is why this is written here rather than baked into the image.
  printf '{"guid":null,"attach":false,"mokEnrolled":false,"osDrive":"%s"}\n' "$ROOT_DEV" > /media/startos/config/setup.json

  echo start > /etc/hostname

  return 0
}

mkdir -p /run/systemd
mount /boot

log "starting (kernel $(uname -r))"
mount / -o remount,ro

beep

if main; then
  log "SUCCESS — resized root filesystem"
  # Clear our own init= hook only on success — a botched resize should be
  # retried, not leave the device booted-but-unresized (which is unusable).
  sed -i 's| init=/usr/lib/startos/scripts/init_resize\.sh||' /boot/grub/grub.cfg
  log "rebooting in 5 seconds"
  sleep 5
else
  log "FAILED — ${FAIL_REASON:-unknown}"
  # Long pause so the failure and the last step marker above stay on screen;
  # the previous build rebooted too fast to read the cause.
  log "pausing 5 minutes before reboot so this is readable"
  sleep 300
fi

sync

umount /boot

reboot -f
