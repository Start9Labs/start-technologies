#!/bin/bash
set -euo pipefail
helper=${1:?provide the production kiosk helper}
fixture=$(mktemp -d)
cp -a /usr/bin/apt "$fixture/apt"
trap 'cp -a "$fixture/apt" /usr/bin/apt; rm -rf "$fixture"' EXIT
printf '#!/bin/sh\nexit 0\n' > /usr/bin/apt
mkdir -p /etc/firefox-esr /usr/lib/firefox-esr /etc/systemd/system/getty.target.wants
printf 'pref("extensions.update.enabled", true);\n' > /etc/firefox-esr/firefox-esr.js
"$helper"
uid=$(id -u kiosk)
cp /home/kiosk/fx-profile/prefs.js "$fixture/prefs.js"
printf 'preserved\n' > /home/kiosk/retry-fixture
printf 'user_pref("stale.retry", true);\n' >> /home/kiosk/fx-profile/prefs.js
"$helper"
[[ $(id -u kiosk) == "$uid" ]]
[[ $(< /home/kiosk/retry-fixture) == preserved ]]
cmp "$fixture/prefs.js" /home/kiosk/fx-profile/prefs.js
[[ $(grep -c 'exec startx' /home/kiosk/.profile) == 1 ]]
[[ -x /home/kiosk/kiosk.sh ]]
[[ -L /etc/systemd/system/getty.target.wants/getty@tty1.service ]]
