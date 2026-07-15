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

# Disassembly runs in the same start9/cargo-zigbuild container the cross-build
# uses (its llvm-objdump targets riscv64), so the host needs no riscv binutils.
# Set RISCV_OBJDUMP to a native riscv64-capable objdump to skip docker.
#
# The decoder follows the binary's ELF arch attributes (like GNU objdump), so
# this check catches exactly its target: compiler-emitted instructions from a
# misconfigured RUSTFLAGS/-mcpu, which always carry matching attributes. Do
# NOT force extensions on via --mattr to "see more": deps ship hand-written
# runtime-dispatched asm (e.g. vendored OpenSSL's RVV routines) whose inline
# constant tables then decode as banned mnemonics — false positives — while
# the asm itself stays out of the attributes and out of this check's scope.
disassemble() {
    if [ -n "${RISCV_OBJDUMP:-}" ]; then
        "$RISCV_OBJDUMP" -d -M no-aliases "$BINARY"
    else
        docker run --rm -v "$(realpath "$BINARY")":/verify/binary:ro \
            start9/cargo-zigbuild \
            sh -c 'set -- /usr/lib/llvm-*/bin/llvm-objdump; exec "$1" -d -M no-aliases /verify/binary'
    fi
}

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

echo "==> Verifying $(basename "$BINARY") against K1 ISA (via ${RISCV_OBJDUMP:-containerized llvm-objdump})..."

# Disassemble once; scan the text for both failure classes. Decoder warnings
# on stderr are suppressed unless the disassembly itself fails — then it's
# the docker/objdump error we need.
DISASM="$(mktemp)"
trap 'rm -f "$DISASM" "$DISASM.err"' EXIT
if ! disassemble 2>"$DISASM.err" > "$DISASM"; then
    cat "$DISASM.err" >&2
    echo "ERROR: disassembly of $BINARY failed" >&2
    exit 1
fi

# An empty disassembly would vacuously pass both scans below.
if [ ! -s "$DISASM" ]; then
    echo "ERROR: disassembly of $BINARY produced no output" >&2
    exit 1
fi

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
