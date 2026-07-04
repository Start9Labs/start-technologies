#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
#
# Copyright (C) 2024 Spacemit Ltd.


echo "gen zip file print"
#set -ex
set -e
[ $# -eq 1 ] || {
    echo "SYNTAX: $0 <file> <partition_table json file> "
    exit 1
}

#why openWRT do not use genimage? i have no idea
#To be compatible with buildroot which use genimage, a conversion is made here.
OUTPUT="$1"
IMGS_DIR=$(dirname $1)

#Give a chance to CI
if [ -z "$BIANBU_LINUX_ARCHIVE" ]; then
    TARGET_IMAGE_ZIP=$OUTPUT
else
    TARGET_IMAGE_ZIP="$BIANBU_LINUX_ARCHIVE.zip"
fi

#used by Spacemit's PC-burning-tool called Titan
#Titan features
# 1. Burning archive's image into storage(nor/flash/emmc/ssd) media on board via USB
# 2. Burning archive's image into SD card for booting card to boot device
# 3. Produce production cards to upgrade device on factory line
# 4. Use archive's image to gen sdcard.img
pack_image_zip() {
    echo "Starting to pack images................................."
    rm -f ${TARGET_IMAGE_ZIP}

    cd ${IMGS_DIR}
    zip ${TARGET_IMAGE_ZIP} \
        fw_dynamic.itb \
        u-boot.itb \
        env.bin \
        bootfs.img \
        rootfs.squashfs \
        partition_*.json \
        fastboot.yaml \
        genimage.cfg \
        -r factory

    #Give a chance to CI
    if [ -n "$BIANBU_LINUX_ARCHIVE_LATEST" ]; then
        ln -sf ${TARGET_IMAGE_ZIP} $BIANBU_LINUX_ARCHIVE_LATEST
    fi

    cd - >/dev/null
 
    echo "Images successfully packed into ${TARGET_IMAGE_ZIP}"
    echo -e "\n"
}

#Pack images in zip
pack_image_zip

