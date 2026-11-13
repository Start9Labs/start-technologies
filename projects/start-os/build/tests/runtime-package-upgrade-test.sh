#!/usr/bin/env bash
set -euo pipefail

ROOT=$(realpath "$(dirname "${BASH_SOURCE[0]}")/../../../..")
TMP=$(mktemp -d)
trap 'rm -rf -- "$TMP"' EXIT
mkdir -p "$TMP/bin"

awk '/^cp / { exit } { print }' "$ROOT/projects/start-os/container-runtime/deb-install.sh" > "$TMP/install.sh"
cat > "$TMP/bin/apt-get" <<'SH'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$APT_LOG"
if [ "$1" = "${FAIL_ACTION:-}" ]; then
    exit 42
fi
SH
chmod +x "$TMP/bin/apt-get"
export PATH="$TMP/bin:$PATH"
export APT_LOG="$TMP/apt.log"

printf '%s\n' 'update' 'upgrade -y' 'install -y curl rsync qemu-user-static nodejs' > "$TMP/expected"
bash "$TMP/install.sh"
diff -u "$TMP/expected" "$APT_LOG"

for FAIL_ACTION in update upgrade; do
    export FAIL_ACTION
    : > "$APT_LOG"
    if bash "$TMP/install.sh"; then
        printf 'FAIL: installer ignored apt-get %s failure\n' "$FAIL_ACTION" >&2
        exit 1
    fi
    if [ "$FAIL_ACTION" = update ]; then
        head -n 1 "$TMP/expected" > "$TMP/expected-failure"
    else
        head -n 2 "$TMP/expected" > "$TMP/expected-failure"
    fi
    diff -u "$TMP/expected-failure" "$APT_LOG"
done

printf 'Runtime packages upgrade before dependency installation; APT failures stop the build\n'
