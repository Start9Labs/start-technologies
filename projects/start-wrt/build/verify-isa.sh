#!/bin/bash
# K1 lacks RVA23 and its kernel cannot emulate misaligned vector accesses.
set -e

BINARY="${1:?usage: verify-isa.sh <binary>}"

if [ ! -f "$BINARY" ]; then
    echo "ERROR: $BINARY not found" >&2
    exit 1
fi

# RISCV_OBJDUMP selects a host decoder instead of the shared builder image.
# Decoding follows ELF attributes, excluding runtime-dispatched assembly.
# --mattr overrides can misdecode inline data as instructions.
disassemble() {
    if [ -n "${RISCV_OBJDUMP:-}" ]; then
        "$RISCV_OBJDUMP" -d -M no-aliases "$BINARY"
    else
        local image
        image=$("$(dirname "${BASH_SOURCE[0]}")/../../../build/rust-builder/image.sh")
        docker run --rm -v "$(realpath "$BINARY")":/verify/binary:ro \
            "$image" \
            sh -c 'set -- /usr/lib/llvm-*/bin/llvm-objdump; exec "$1" -d -M no-aliases /verify/binary'
    fi
}

BANNED_RE='\b(czero\.(eqz|nez)|fli\.[sdhq]|fround(nx)?\.[sdhq]|fmaxm\.[sdhq]|fminm\.[sdhq]|fleq\.[sdhq]|fltq\.[sdhq]|fcvtmod\.w\.d|fmvh\.x\.d|fmvp\.d\.x|amocas\.[wdq]|c\.mop\.[0-9]+|mop\.r\.[0-9]+|mop\.rr\.[0-9]+|f(add|sub|mul|div|sqrt|min|max|madd|msub|nmadd|nmsub|sgnj|sgnjn|sgnjx|eq|lt|le|class|mv|cvt)\.h|c\.(lbu|lhu|lh|sb|sh|zext\.[bhw]|sext\.[bh]|not|mul))\b'

VSET_RE='\bvset(i?vli|vl)\b'

echo "==> Verifying $(basename "$BINARY") against K1 ISA (via ${RISCV_OBJDUMP:-containerized llvm-objdump})..."

DISASM="$(mktemp)"
trap 'rm -f "$DISASM" "$DISASM.err"' EXIT
if ! disassemble 2>"$DISASM.err" > "$DISASM"; then
    cat "$DISASM.err" >&2
    echo "ERROR: disassembly of $BINARY failed" >&2
    exit 1
fi

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
