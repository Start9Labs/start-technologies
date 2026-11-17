#!/bin/bash
set -euo pipefail
installer=${1:?provide the production mokutil image installer}
fixture=$(mktemp -d)
trap 'rm -rf "$fixture"' EXIT
cat > "$fixture/dpkg-buildpackage" <<'EOF'
#!/bin/bash
printf 'attempt\n' >> "$MOK_BUILD_ATTEMPTS"
if [[ ! -f "$MOK_BUILD_FAILED" ]]; then
    touch "$MOK_BUILD_FAILED"
    exit 74
fi
exec /usr/bin/dpkg-buildpackage "$@"
EOF
chmod +x "$fixture/dpkg-buildpackage"
export MOK_BUILD_ATTEMPTS="$fixture/attempts" MOK_BUILD_FAILED="$fixture/failed"
status=0
PATH="$fixture:$PATH" "$installer" || status=$?
[[ $status == 74 ]]
dpkg-query -W -f='${binary:Package}\n' > "$fixture/packages.after-failure"
cmp /packages.before "$fixture/packages.after-failure"
diff -r /apt-sources.before /etc/apt/sources.list.d
PATH="$fixture:$PATH" "$installer"
[[ $(wc -l < "$fixture/attempts") == 2 ]]
sha256sum /usr/bin/mokutil > "$fixture/binary.before-reentry"
PATH="$fixture:$PATH" "$installer"
[[ $(wc -l < "$fixture/attempts") == 2 ]]
sha256sum --check "$fixture/binary.before-reentry"
printf 'Native installer failure cleanup, retry and completed-stage re-entry passed\n'
