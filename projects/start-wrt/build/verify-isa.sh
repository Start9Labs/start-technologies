#!/bin/bash
# Verify a RISC-V binary is safe to run on the SpaceMiT K1.
#
# Two failure classes are rejected:
#  - RVA23-only extensions (Zicond, Zfa, Zacas, Zfh, Zcmop, Zimop, vector
#    crypto): absent from K1's RVA22 baseline, so they SIGILL.
#  - Base RISC-V Vector (V 1.0): K1 implements it, but traps on misaligned
#    vector-element accesses that the Bianbu kernel does not emulate, so
#    auto-vectorised code SIGBUSes (BUS_ADRALN) at runtime. Policy: no V.
set -e

BINARY="${1:?usage: verify-isa.sh <binary>}"

if [ ! -f "$BINARY" ]; then
    echo "ERROR: $BINARY not found" >&2
    exit 1
fi

OBJDUMP=""
for candidate in \
    "${RISCV_OBJDUMP:-}" \
    riscv64-linux-gnu-objdump \
    riscv64-openwrt-linux-musl-objdump \
    riscv64-unknown-linux-musl-objdump; do
    if [ -n "$candidate" ] && command -v "$candidate" >/dev/null 2>&1; then
        OBJDUMP="$candidate"
        break
    fi
done
if [ -z "$OBJDUMP" ]; then
    for f in openwrt/staging_dir/toolchain-riscv64*/bin/*-objdump; do
        if [ -x "$f" ]; then
            OBJDUMP="$f"
            break
        fi
    done
fi
if [ -z "$OBJDUMP" ]; then
    echo "ERROR: no riscv64 objdump found (install binutils-riscv64-linux-gnu or set RISCV_OBJDUMP)" >&2
    exit 1
fi

# RVA23-only instructions that SIGILL on K1:
#   Zicond:  czero.eqz, czero.nez
#   Zfa:     fli.*, fround(nx)?.*, fmaxm.*, fminm.*, fleq.*, fltq.*,
#            fcvtmod.w.d, fmvh.x.d, fmvp.d.x
#   Zacas:   amocas.w/d/q
#   Zfh:     flh, fsh, f*.h (half-precision FP)
#   Zcmop:   c.mop.N
#   Zimop:   mop.r.N, mop.rr.N
#   Zcb:     c.lbu, c.lhu, c.lh, c.sb, c.sh, c.zext.b, c.sext.b,
#            c.zext.h, c.sext.h, c.zext.w, c.not, c.mul
BANNED_RE='\b(czero\.(eqz|nez)|fli\.[sdhq]|fround(nx)?\.[sdhq]|fmaxm\.[sdhq]|fminm\.[sdhq]|fleq\.[sdhq]|fltq\.[sdhq]|fcvtmod\.w\.d|fmvh\.x\.d|fmvp\.d\.x|amocas\.[wdq]|c\.mop\.[0-9]+|mop\.r\.[0-9]+|mop\.rr\.[0-9]+|f(add|sub|mul|div|sqrt|min|max|madd|msub|nmadd|nmsub|sgnj|sgnjn|sgnjx|eq|lt|le|class|mv|cvt)\.h|c\.(lbu|lhu|lh|sb|sh|zext\.[bhw]|sext\.[bh]|not|mul))\b'

# RISC-V Vector (RVV / "+v") instructions. K1 implements V 1.0 so these do
# not SIGILL — but the CPU traps on misaligned vector-element accesses, which
# the Bianbu kernel does not emulate, so auto-vectorised code SIGBUSes at
# runtime (BUS_ADRALN). Every RVV sequence starts with a vset{i,}vl{i,}, so
# detecting those catches any vectorised code.
VSET_RE='\bvset(i?vli|vl)\b'

echo "==> Verifying $(basename "$BINARY") against K1 ISA (via $(basename "$OBJDUMP"))..."

# Disassemble once; scan the text for both failure classes.
DISASM="$(mktemp)"
trap 'rm -f "$DISASM"' EXIT
"$OBJDUMP" -d -M no-aliases "$BINARY" 2>/dev/null > "$DISASM"

FOUND=$(grep -oE "$BANNED_RE" "$DISASM" | sort -u || true)
if [ -n "$FOUND" ]; then
    echo "ERROR: $BINARY contains RVA23-only instructions not supported by SpaceMiT K1:" >&2
    echo "$FOUND" | sed 's/^/    /' >&2
    echo "" >&2
    echo "Check RUSTFLAGS in build/build-rust.sh and the cargo-zigbuild image default." >&2
    exit 1
fi

VFOUND=$(grep -oE "$VSET_RE" "$DISASM" | sort -u || true)
if [ -n "$VFOUND" ]; then
    echo "ERROR: $BINARY contains RISC-V Vector instructions:" >&2
    echo "$VFOUND" | sed 's/^/    /' >&2
    echo "" >&2
    echo "RVV code SIGBUSes on the SpaceMiT K1 — misaligned vector access is" >&2
    echo "not emulated by the Bianbu kernel. Remove '+v' from build/build-rust.sh," >&2
    echo "build/zigcc-k1.sh, and build/zigcxx-k1.sh." >&2
    exit 1
fi

echo "    OK — no RVA23-only or vector instructions"
