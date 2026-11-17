#!/bin/bash
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel)
SOURCE_MAKEFILE=${1:-$ROOT/projects/start-wrt/openwrt/package/system/ca-certificates/Makefile}
[ -f "$SOURCE_MAKEFILE" ] || { echo 'Prepare the OpenWrt tree before this test.' >&2; exit 1; }
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/include" "$TMP/fixture" "$TMP/output"
touch "$TMP/rules.mk"

openssl req -x509 -newkey rsa:2048 -nodes -days 1 \
    -subj '/CN=emSign ECC Root CA - G3' \
    -keyout "$TMP/g3.key" -out "$TMP/fixture/emSign_ECC_Root_CA_-_G3.crt" 2>"$TMP/openssl.log"
openssl req -x509 -newkey rsa:2048 -nodes -days 1 \
    -subj '/CN=Unrelated Test Root' \
    -keyout "$TMP/allowed.key" -out "$TMP/fixture/allowed.crt" 2>>"$TMP/openssl.log"

cat > "$TMP/include/package.mk" <<'EOF'
define Build/Install/Default
	mkdir -p $(PKG_INSTALL_DIR)/usr/share/ca-certificates/mozilla
	cp $(FIXTURE_DIR)/*.crt $(PKG_INSTALL_DIR)/usr/share/ca-certificates/mozilla/
endef
EOF
cat > "$TMP/test.mk" <<EOF
TOPDIR:=$TMP
INCLUDE_DIR:=$TMP/include
FIXTURE_DIR:=$TMP/fixture
PKG_INSTALL_DIR:=$TMP/staging
INSTALL_DIR:=mkdir -p
INSTALL_DATA:=cp
LN:=ln -s
include $SOURCE_MAKEFILE
.PHONY: test
define RunTrustStoreTest
test:
	\$(Build/Install)
	\$(call Package/ca-certificates/install,$TMP/output/hashed)
	\$(call Package/ca-bundle/install,$TMP/output/bundle)
endef
\$(eval \$(RunTrustStoreTest))
EOF
make -s -f "$TMP/test.mk" test

[ ! -e "$TMP/output/hashed/etc/ssl/certs/emSign_ECC_Root_CA_-_G3.crt" ]
cmp "$TMP/fixture/allowed.crt" "$TMP/output/hashed/etc/ssl/certs/allowed.crt"
for store in hashed bundle; do
    args=(-CApath "$TMP/output/hashed/etc/ssl/certs")
    if [ "$store" = bundle ]; then
        args=(-CAfile "$TMP/output/bundle/etc/ssl/certs/ca-certificates.crt")
    fi
    openssl verify "${args[@]}" "$TMP/fixture/allowed.crt"
    if openssl verify "${args[@]}" "$TMP/fixture/emSign_ECC_Root_CA_-_G3.crt" >"$TMP/rejected.log" 2>&1; then
        echo "$store still trusts G3" >&2
        exit 1
    fi
done
printf 'Both OpenWrt CA outputs reject G3 and preserve unrelated trust.\n'
