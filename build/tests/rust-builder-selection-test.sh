#!/bin/bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo_root"
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
mkdir "$scratch/bin"
export DOCKER_LOG="$scratch/docker.log"
export OBJDUMP_LOG="$scratch/objdump.log"
export PATH="$scratch/bin:$PATH"
export ENVIRONMENT=dev-unstable
unset RUST_BUILDER_IMAGE FMT_NATIVE RISCV_OBJDUMP

cat > "$scratch/bin/docker" <<'MOCK'
#!/bin/bash
printf '%s\n' '---' "$@" >> "$DOCKER_LOG"
if [[ "$*" == *'/verify/binary'* ]]; then
    printf '00000000 <probe>:\n    0: addi a0, a0, 1\n'
fi
MOCK
cat > "$scratch/bin/objdump" <<'MOCK'
#!/bin/bash
printf '%s\n' "$@" >> "$OBJDUMP_LOG"
printf '00000000 <probe>:\n    0: addi a0, a0, 1\n'
MOCK
chmod +x "$scratch/bin/docker" "$scratch/bin/objdump"

assert_log_contains() {
    if ! grep -Fxq -- "$1" "$DOCKER_LOG"; then
        printf 'Missing Docker argument: %s\n' "$1" >&2
        exit 1
    fi
}

image=$(./build/rust-builder/image.sh)
if ! [[ "$image" =~ ^ghcr\.io/start9labs/cargo-zigbuild@sha256:[0-9a-f]{64}$ ]]; then
    printf 'Builder image is not pinned: %s\n' "$image" >&2
    exit 1
fi
custom=example.invalid/custom-builder:test
test "$(RUST_BUILDER_IMAGE="$custom" ./build/rust-builder/image.sh)" = "$custom"
test "$(cd "$scratch" && "$repo_root/build/rust-builder/image.sh")" = "$image"

for alias_file in build/builder-alias.sh projects/start-wrt/build/builder-alias.sh; do
    : > "$DOCKER_LOG"
    (
        cd "$scratch"
        bash -ec 'shopt -s expand_aliases; source "$1"; eval "rust-zig-builder cargo --version"' -- "$repo_root/$alias_file"
    )
    assert_log_contains "$image"
    : > "$DOCKER_LOG"
    bash -ec 'RUST_BUILDER_IMAGE=$2; shopt -s expand_aliases; source "$1"; eval "rust-zig-builder cargo --version"' -- "$repo_root/$alias_file" "$custom"
    assert_log_contains "$custom"
done

: > "$DOCKER_LOG"
bash shared-libs/crates/start-core/run-tests.sh
assert_log_contains "RUST_BUILDER_IMAGE=$image"
assert_log_contains start9/start-core-testenv
assert_log_contains --features=test,ts,dev,unstable

: > "$DOCKER_LOG"
RUST_BUILDER_IMAGE=start9/start-core-testenv bash shared-libs/crates/start-core/run-tests.sh
assert_log_contains "RUST_BUILDER_IMAGE=$image"
assert_log_contains start9/start-core-testenv

: > "$DOCKER_LOG"
RUST_BUILDER_IMAGE="$custom" bash shared-libs/crates/start-core/run-tests.sh
assert_log_contains "$custom"
if grep -Fxq buildx "$DOCKER_LOG"; then
    printf 'A prebuilt core test image unexpectedly triggers a build\n' >&2
    exit 1
fi

: > "$DOCKER_LOG"
bash projects/start-wrt/build/run-tests.sh
assert_log_contains "$image"
assert_log_contains startwrt-core
assert_log_contains uciedit_macros

: > "$DOCKER_LOG"
RUST_BUILDER_IMAGE="$custom" bash projects/start-wrt/build/run-tests.sh
assert_log_contains "$custom"

: > "$DOCKER_LOG"
(
    cd shared-libs/crates/start-core
    "$repo_root/build/fmt/run-fmt.sh" cargo fmt --version
)
assert_log_contains "RUST_BUILDER_IMAGE=$image"
assert_log_contains /workdir/shared-libs/crates/start-core
assert_log_contains RUSTUP_TOOLCHAIN=nightly-2026-05-28
assert_log_contains --load

: > "$DOCKER_LOG"
RUST_BUILDER_IMAGE="$custom" ./build/fmt/run-fmt.sh cargo fmt --version
assert_log_contains "RUST_BUILDER_IMAGE=$custom"

: > "$DOCKER_LOG"
test "$(FMT_NATIVE=1 ./build/fmt/run-fmt.sh printenv RUSTUP_TOOLCHAIN)" = nightly-2026-05-28
test ! -s "$DOCKER_LOG"

: > "$DOCKER_LOG"
touch "$scratch/probe.o"
bash projects/start-wrt/build/verify-isa.sh "$scratch/probe.o"
assert_log_contains "$image"

: > "$DOCKER_LOG"
RUST_BUILDER_IMAGE="$custom" bash projects/start-wrt/build/verify-isa.sh "$scratch/probe.o"
assert_log_contains "$custom"

: > "$DOCKER_LOG"
RISCV_OBJDUMP="$scratch/bin/objdump" bash projects/start-wrt/build/verify-isa.sh "$scratch/probe.o"
test ! -s "$DOCKER_LOG"
grep -Fxq -- "$scratch/probe.o" "$OBJDUMP_LOG"

printf 'Builder selection, overrides, derived images, and K1 disassembly checks passed.\n'
