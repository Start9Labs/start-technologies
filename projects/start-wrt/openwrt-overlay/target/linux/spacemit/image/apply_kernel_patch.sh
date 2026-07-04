#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
#
# Copyright (C) 2024 Spacemit Ltd.


echo "apply kernel patchs!"

pwd

./scripts/patch-kernel.sh build_dir/target-riscv64_riscv64_musl_DEVICE_debX/linux-spacemit_k1-sbc/linux-6.6.36/ target/linux/generic/backport-6.6/
./scripts/patch-kernel.sh build_dir/target-riscv64_riscv64_musl_DEVICE_debX/linux-spacemit_k1-sbc/linux-6.6.36/ target/linux/generic/pending-6.6/
./scripts/patch-kernel.sh build_dir/target-riscv64_riscv64_musl_DEVICE_debX/linux-spacemit_k1-sbc/linux-6.6.36/ target/linux/generic/hack-6.6
./scripts/patch-kernel.sh build_dir/target-riscv64_riscv64_musl_DEVICE_debX/linux-spacemit_k1-sbc/linux-6.6.36/ target/linux/spacemit/patches-6.6/

echo "apply kernel patchs end"
