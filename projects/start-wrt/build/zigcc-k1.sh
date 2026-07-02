#!/bin/sh
# cargo-zigbuild-style wrapper pinned to the SpaceMiT K1 (BPI-F3) ISA.
# Used as both the compiler (CC) and linker (RUSTC_LINKER) for the
# riscv64gc-unknown-linux-musl target to keep ABI and instruction set
# consistent across rustc output, C deps, and zig's runtime objects.
# Deliberately NO `+v` in -mcpu: K1 traps on misaligned vector accesses
# (Bianbu 6.6 kernel does not emulate them) → SIGBUS/BUS_ADRALN from any
# auto-vectorised loop. Keep the C-dep ISA in lockstep with build-rust.sh.
exec /usr/local/cargo/bin/cargo-zigbuild zig cc -- \
    -g -fno-sanitize=all \
    -mcpu=generic_rv64+m+a+f+d+c+zicsr+zifencei+zicntr+zihpm+ziccif+ziccamoa+zicclsm+ziccrse+za64rs+zihintpause+zic64b+zicbom+zicbop+zicboz+zba+zbb+zbs+zkt+zihintntl+zawrs \
    -target riscv64-linux-musl \
    "$@"
