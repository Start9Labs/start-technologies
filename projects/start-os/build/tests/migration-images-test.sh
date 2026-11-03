#!/bin/bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../../.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/bin" "$TMP/images"
export CALLS="$TMP/calls" FAIL_SAVE="$TMP/fail-save"
cat > "$TMP/bin/docker" <<'EOF'
#!/bin/bash
set -euo pipefail
printf '%s\n' "$*" >> "$CALLS"
case "$1" in
    pull) ;;
    save)
        printf '%s %s\n' "$3" "$4" > "$6"
        if [ -e "$FAIL_SAVE" ]; then
            rm "$FAIL_SAVE"
            exit 42
        fi
        ;;
    *) exit 1 ;;
esac
EOF
chmod +x "$TMP/bin/docker"
export PATH="$TMP/bin:$PATH"

for NAME in start9_compat_latest start9_utils_latest tonistiigi_binfmt_latest; do
    touch "$TMP/images/$NAME.tar"
done

touch "$FAIL_SAVE"
if bash "$ROOT/projects/start-os/build/save-migration-images.sh" "$TMP/images"; then
    echo "Failed export unexpectedly succeeded" >&2
    exit 1
fi
test ! -f "$TMP/images/x86_64/start9_compat_latest.tar"
: > "$CALLS"
bash "$ROOT/projects/start-os/build/save-migration-images.sh" "$TMP/images"

for ARCH in x86_64 aarch64; do
    case "$ARCH" in
        x86_64) PLATFORM=linux/amd64 ;;
        aarch64) PLATFORM=linux/arm64 ;;
    esac
    for NAME in compat utils; do
        grep -Fx "$PLATFORM start9/$NAME:latest" "$TMP/images/$ARCH/start9_${NAME}_latest.tar"
        grep -Fx "pull --platform $PLATFORM start9/$NAME:latest" "$CALLS"
        grep -Fx "save --platform $PLATFORM start9/$NAME:latest -o $TMP/images/$ARCH/start9_${NAME}_latest.tar.tmp" "$CALLS"
    done
done
test "$(wc -l < "$CALLS")" -eq 8
test "$(find "$TMP/images" -type f | wc -l)" -eq 4
: > "$CALLS"
bash "$ROOT/projects/start-os/build/save-migration-images.sh" "$TMP/images"
test ! -s "$CALLS"
if grep -qx podman "$ROOT/projects/start-os/build/dpkg-deps/depends"; then
    echo "StartOS still depends on Podman" >&2
    exit 1
fi
if grep -q '/etc/containers' "$ROOT/projects/start-os/debian/postinst"; then
    echo "postinst still configures the removed container engine" >&2
    exit 1
fi
echo "Migration image tests passed"
