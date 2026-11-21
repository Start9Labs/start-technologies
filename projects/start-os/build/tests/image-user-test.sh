#!/bin/bash
set -euo pipefail
helper=${1:?provide the production image account helper}
fixture=$(mktemp -d)
trap 'rm -rf "$fixture"' EXIT
groupadd -f startos
groupadd -f sudo
groupadd -f systemd-journal
mkdir -p /etc/sudoers.d
cat > "$fixture/chpasswd" <<'EOF'
#!/bin/bash
if [[ ! -f "$ACCOUNT_FAILURE" ]]; then
    touch "$ACCOUNT_FAILURE"
    exit 73
fi
exec /usr/sbin/chpasswd "$@"
EOF
chmod +x "$fixture/chpasswd"
status=0
PATH="$fixture:$PATH" ACCOUNT_FAILURE="$fixture/failed" "$helper" dev || status=$?
[[ $status == 73 ]]
uid=$(id -u start9)
printf 'preserved\n' > ~start9/retry-fixture
PATH="$fixture:$PATH" ACCOUNT_FAILURE="$fixture/failed" "$helper" dev
[[ $(id -u start9) == "$uid" ]]
[[ $(< ~start9/retry-fixture) == preserved ]]
[[ $(passwd -S start9 | awk '{print $2}') == P ]]
"$helper" ''
[[ $(passwd -S start9 | awk '{print $2}') == L ]]
userdel -r start9
useradd --no-create-home start9
if "$helper" dev; then
    echo 'Incomplete account unexpectedly accepted' >&2
    exit 1
fi
userdel start9
