#!/bin/bash
set -euo pipefail

if [ "${START9_RVA23:-1}" != 0 ]; then
  export CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_MUSL_RUSTFLAGS="${CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_MUSL_RUSTFLAGS--C target-cpu=generic-rv64 -C target-feature=+v,+zicsr,+zifencei,+zicntr,+zihpm,+ziccif,+ziccamoa,+zicclsm,+ziccrse,+za64rs,+zihintpause,+zic64b,+zicbom,+zicbop,+zicboz,+zba,+zbb,+zbs,+zfhmin,+zkt,+zvfhmin,+zvbb,+zvkt,+zihintntl,+zicond,+zimop,+zcmop,+zcb,+zfa,+zawrs,+supm}"
fi

exec "$@"
