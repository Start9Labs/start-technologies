#!/bin/bash
set -euo pipefail

if [ "${DISABLE_RVA23:-0}" != 1 ]; then
  export CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_MUSL_RUSTFLAGS="${CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_MUSL_RUSTFLAGS--C target-feature=+v,+zicsr,+zifencei,+zicntr,+zihpm,+ziccif,+ziccamoa,+zicclsm,+ziccrse,+za64rs,+zihintpause,+zic64b,+zicbom,+zicbop,+zicboz,+zba,+zbb,+zbs,+zfhmin,+zkt,+zvfhmin,+zvbb,+zvkt,+zihintntl,+zicond,+zimop,+zcmop,+zcb,+zfa,+zawrs,+supm}"
fi

exec "$@"
