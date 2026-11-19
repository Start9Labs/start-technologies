#!/bin/bash
set -euo pipefail
ROOT=$(realpath "$(dirname "${BASH_SOURCE[0]}")/../../../..")
WORK=$(mktemp -d)
trap 'rm -rf -- "$WORK"' EXIT
awk '
/^KVER_ALL=/ { found=1 }
found && /^if \[ "\$\{IB_SUITE\}" = trixie \]/ { exit }
found { gsub(/\\\$/, "$"); print }
' "$ROOT/projects/start-os/build/image-recipe/build.sh" > "$WORK/kbuild.sh"
mkdir "$WORK/bin"
cat > "$WORK/bin/ls" <<'EOF'
#!/bin/bash
printf '/boot/vmlinuz-target-abi\n'
EOF
cat > "$WORK/bin/apt-cache" <<'EOF'
#!/bin/bash
[ "$*" = 'depends linux-headers-target-abi' ] || exit 1
printf '  Depends: linux-kbuild-from-header-metadata\n'
EOF
cat > "$WORK/bin/apt-get" <<'EOF'
#!/bin/bash
printf '%s\n' "$*" > "$RESULTS_DIR/kbuild-installed"
EOF
chmod +x "$WORK/bin/"*
RESULTS_DIR="$WORK" PATH="$WORK/bin:$PATH" bash "$WORK/kbuild.sh"
grep -Fxq 'install -y linux-kbuild-from-header-metadata' "$WORK/kbuild-installed"
printf 'target kbuild selection test passed\n'
