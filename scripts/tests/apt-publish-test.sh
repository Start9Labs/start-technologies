#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
TMP=$(mktemp -d)
export GNUPGHOME="$TMP/gnupg"
mkdir -m 700 "$GNUPGHOME"
trap 'gpgconf --kill all 2> /dev/null || true; rm -rf "$TMP"' EXIT

fail() {
    echo "FAIL: $*" >&2
    exit 1
}

gpg --batch --quiet --passphrase '' --quick-generate-key 'Test <test@example.invalid>' ed25519 sign never 2> /dev/null
gpg --batch --quiet --export > "$TMP/keyring.gpg"
GPG_KEY_ID=$(gpg --batch --list-keys --with-colons | awk -F: '/^pub/ { print $5; exit }')
export GPG_KEY_ID
export APT_KEYRING="$TMP/keyring.gpg"
export S3_BUCKET=bucket
unset S3_ACCESS_KEY S3_SECRET_KEY GPG_PRIVATE_KEY

export FAKE_S3="$TMP/s3"
mkdir -p "$TMP/bin" "$FAKE_S3/bucket"
cat > "$TMP/bin/s3cmd" <<'FAKE'
#!/usr/bin/env bash
set -euo pipefail
cmd=$1
shift
paths=()
for arg in "$@"; do
    case "$arg" in
        --*) ;;
        *) paths+=("$arg") ;;
    esac
done
local_path() { echo "${1/s3:\/\//$FAKE_S3/}"; }
case "$cmd" in
    ls)
        [ -d "$(local_path "${paths[0]}")" ] || exit 0
        find "$(local_path "${paths[0]}")" -type f
        ;;
    sync)
        src=$(local_path "${paths[0]}")
        dest=$(local_path "${paths[1]}")
        case "${paths[1]}" in s3://*) rm -rf "$dest" ;; esac
        mkdir -p "$dest"
        cp -a "$src/." "$dest/"
        ;;
    *) exit 2 ;;
esac
FAKE
chmod +x "$TMP/bin/s3cmd"
export PATH="$TMP/bin:$PATH"

make_deb() {
    local name=$1 version=$2 dir="$TMP/build/$1-$2"
    mkdir -p "$dir/DEBIAN"
    cat > "$dir/DEBIAN/control" <<CONTROL
Package: $name
Version: $version
Architecture: amd64
Maintainer: Test <test@example.invalid>
Description: test package
CONTROL
    dpkg-deb --root-owner-group --build "$dir" "$TMP/$name-$version.deb" > /dev/null
    echo "$TMP/$name-$version.deb"
}

publish() {
    "$ROOT/debian/publish.sh" "$@" > "$TMP/publish.log" 2>&1
}

expect_refused() {
    local reason=$1
    shift
    rm -rf "$TMP/before"
    cp -a "$FAKE_S3" "$TMP/before"
    if publish "$@"; then
        fail "$reason: publish succeeded"
    fi
    grep -q 'Refusing to publish' "$TMP/publish.log" || {
        cat "$TMP/publish.log" >&2
        fail "$reason: failed for another reason"
    }
    diff -r "$TMP/before" "$FAKE_S3" > /dev/null || fail "$reason: bucket changed"
}

restore() {
    rm -rf "$FAKE_S3"
    cp -a "$TMP/good" "$FAKE_S3"
}

BUCKET="$FAKE_S3/bucket"
PACKAGES="$BUCKET/dists/stable/main/binary-amd64/Packages"
A1=$(make_deb pkg-a 1.0.0)
A2=$(make_deb pkg-a 2.0.0)
B1=$(make_deb pkg-b 1.0.0)
EVIL=$(make_deb evil 9.9.9)

publish "$A1" || { cat "$TMP/publish.log" >&2; fail 'first publish'; }
publish "$B1" || { cat "$TMP/publish.log" >&2; fail 'publish alongside a verified pool'; }
grep -q 'pool/main/p/pkg-a/pkg-a_1.0.0_amd64.deb' "$PACKAGES" || fail 'existing package dropped from the index'
grep -q 'pool/main/p/pkg-b/pkg-b_1.0.0_amd64.deb' "$PACKAGES" || fail 'new package missing from the index'
cp -a "$FAKE_S3" "$TMP/good"

mkdir -p "$BUCKET/pool/main/e/evil"
cp "$EVIL" "$BUCKET/pool/main/e/evil/evil_9.9.9_amd64.deb"
expect_refused 'unindexed pool file' "$A2"
restore

cp "$EVIL" "$BUCKET/pool/main/p/pkg-b/pkg-b_1.0.0_amd64.deb"
expect_refused 'modified indexed deb' "$A2"
restore

echo >> "$PACKAGES"
expect_refused 'modified Packages index' "$A2"
restore

sed -i 's/^Suite: .*/Suite: tampered/' "$BUCKET/dists/stable/InRelease"
expect_refused 'modified InRelease' "$A2"
restore

STORED="$TMP/stored.pgp"
gpg --batch --yes --store --output "$STORED" "$BUCKET/dists/stable/Release"

cp "$STORED" "$BUCKET/dists/stable/InRelease"
expect_refused 'unsigned OpenPGP InRelease' "$A2"
restore

cat "$STORED" "$TMP/good/bucket/dists/stable/InRelease" > "$BUCKET/dists/stable/InRelease"
expect_refused 'unsigned OpenPGP before a signed InRelease' "$A2"
restore

verify_alpha_release_on() {
    local source=$1
    (
        source <(awk '/^SUBCOMMAND=/{exit} {print}' "$ROOT/scripts/manage-release.sh")
        REPO_ROOT="$ROOT"
        curl() { cp "$source" "$TMP/alpha/InRelease"; }
        rm -rf "$TMP/alpha"
        mkdir "$TMP/alpha"
        verify_alpha_release "$APT_KEYRING" "$TMP/alpha"
    )
}

verify_alpha_release_on "$BUCKET/dists/stable/InRelease" > /dev/null 2>&1 || fail 'alpha release check refused a signed InRelease'
if verify_alpha_release_on "$STORED" > /dev/null 2>&1; then
    fail 'alpha release check accepted an unsigned InRelease'
fi

rm "$BUCKET/dists/stable/InRelease"
expect_refused 'pool without an InRelease' "$A2"
restore

printf 'Package: evil\n' > "$BUCKET/dists/stable/main/binary-amd64/Packages.xz"
publish "$A2" || { cat "$TMP/publish.log" >&2; fail 'publish with a stray index'; }
[ ! -e "$BUCKET/dists/stable/main/binary-amd64/Packages.xz" ] || fail 'stray index survived'
grep -q 'Packages.xz' "$BUCKET/dists/stable/Release" && fail 'stray index was signed'
restore

mkdir -p "$BUCKET/pool/main/p/pkg-a"
cp "$A2" "$BUCKET/pool/main/p/pkg-a/pkg-a_2.0.0_amd64.deb"
publish "$A2" || { cat "$TMP/publish.log" >&2; fail 'retry of an interrupted publish'; }
grep -q 'pkg-a_2.0.0_amd64.deb' "$PACKAGES" || fail 'retry did not index the package'
[ ! -e "$BUCKET/pool/main/p/pkg-a/pkg-a_1.0.0_amd64.deb" ] || fail 'retry kept the old version'

echo 'apt-publish-test: ok'
