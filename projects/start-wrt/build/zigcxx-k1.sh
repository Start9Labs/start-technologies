#!/bin/sh
# See zigcc-k1.sh — C++ variant.
exec /usr/local/cargo/bin/cargo-zigbuild zig c++ -- \
    -g -fno-sanitize=all \
    -mcpu=generic_rv64+m+a+f+d+c+zicsr+zifencei+zicntr+zihpm+ziccif+ziccamoa+zicclsm+ziccrse+za64rs+zihintpause+zic64b+zicbom+zicbop+zicboz+zba+zbb+zbs+zkt+zihintntl+zawrs \
    -target riscv64-linux-musl \
    "$@"
