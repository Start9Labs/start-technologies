#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
#
# Copyright (C) 2024 Spacemit Ltd.


echo "gen sdcard image print"
#set -ex
set -e
[ $# -eq 5 ] || {
    echo "SYNTAX: $0 <file> <partition_table json file> "
    exit 1
}

#why openWRT do not use genimage? i have no idea
#To be compatible with buildroot which use genimage, a conversion is made here.
OUTPUT="$1"
IMGS_DIR=$(dirname $1)

if echo "$1" |grep -q '\.zip$'; then
    OUTPUT=$(echo "$1" |sed 's/\.zip$//')-sdcard.img
    echo "new sdcard image name:$OUTPUT"
else
    OUTPUT="$1-sdcard.img"
    echo "set image name to sdcard.img"
fi


#Bootinfo contains only the first 80 bytes of valid data.
BOOTINFO=${IMGS_DIR}/$(jq '.partitions[] | select(.name == "bootinfo") | .image' "$4" | sed 's/["]//g')

FSBL=${IMGS_DIR}/$(jq '.partitions[] | select(.name == "fsbl") | .image' "$4" | sed 's/["]//g')
FSBL_SIZE=$(jq '.partitions[] | select(.name == "fsbl") | .size' "$4" | sed 's/["kK]//g') 
FSBL_OFFSET=$(jq '.partitions[] | select(.name == "fsbl") | .offset' "$4" | sed 's/["kK]//g') 

#if flash env.bin is optional, but env part must be fixed offset at 512k
UENV=${IMGS_DIR}/$(jq '.partitions[] | select(.name == "env") | .image' "$4" | sed 's/["]//g')
UENV_SIZE=$(jq '.partitions[] | select(.name == "env") | .size' "$4" | sed 's/["kK]//g')
UENV_OFFSET=$(jq '.partitions[] | select(.name == "env") | .offset' "$4" | sed 's/["kK]//g')

OPENSBI=${IMGS_DIR}/$(jq '.partitions[] | select(.name == "opensbi") | .image' "$4" | sed 's/["]//g')
OPENSBI_SIZE=$(jq '.partitions[] | select(.name == "opensbi") | .size' "$4" | sed 's/["kK]//g')

UBOOT=${IMGS_DIR}/$(jq '.partitions[] | select(.name == "uboot") | .image' "$4" | sed 's/["]//g')
UBOOT_SIZE=$(jq '.partitions[] | select(.name == "uboot") | .size' "$4" | sed 's/["mM]//g')

BOOTFS="$2"
#${IMGS_DIR}/$(jq '.partitions[] | select(.name == "bootfs") | .image' "$2" | sed 's/["]//g')
BOOTFS_SIZE=$(jq '.partitions[] | select(.name == "bootfs") | .size' "$4" | sed 's/["mM]//g')

ROOTFS="$3"
#${IMGS_DIR}/$(jq '.partitions[] | select(.name == "rootfs") | .image' "$2" | sed 's/["]//g')

ROOTFS_SIZE=$5
ROOTFS_DATA_SIZE=$(jq -r '.partitions[] | select(.name=="rootfs_data") | .size' "$4" | sed 's/[mM]//')
head=4
sect=63

#unit is kbytes default
set $(ptgen -o $OUTPUT -v -g -h $head -s $sect \
    -N fsbl -p $FSBL_SIZE@$FSBL_OFFSET \
    -N env -p $UENV_SIZE@$UENV_OFFSET \
    -N opensbi -p $OPENSBI_SIZE \
    -N uboot -p ${UBOOT_SIZE}M \
    -N bootfs -p ${BOOTFS_SIZE}M \
    -N rootfs -p ${ROOTFS_SIZE}M \
    -N rootfs_data -p ${ROOTFS_DATA_SIZE}M)

OPENSBI_OFFSET=$(($5 / 1024))
UBOOT_OFFSET=$(($7 / 1024))
BOOTFS_OFFSET=$(($9 / 1024))
ROOTFS_OFFSET=$((${11} / 1024))

#Bootinfo contains only the first 80 bytes of valid data.
dd bs=80   if="$BOOTINFO" of="$OUTPUT" seek=0 count=1         conv=notrunc
dd bs=1024 if="$FSBL"     of="$OUTPUT" seek=${FSBL_OFFSET}    conv=notrunc
dd bs=1024 if="$UENV"     of="$OUTPUT" seek=${UENV_OFFSET}    conv=notrunc
dd bs=1024 if="$OPENSBI"  of="$OUTPUT" seek=${OPENSBI_OFFSET} conv=notrunc
dd bs=1024 if="$UBOOT"    of="$OUTPUT" seek=${UBOOT_OFFSET}   conv=notrunc
dd bs=1024 if="$BOOTFS"   of="$OUTPUT" seek=${BOOTFS_OFFSET}  conv=notrunc
dd bs=1024 if="$ROOTFS"   of="$OUTPUT" seek=${ROOTFS_OFFSET}  conv=notrunc

echo "$OUTPUT successfully generated"

