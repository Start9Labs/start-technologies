#!/bin/bash
set -euo pipefail

directory=$(cd "$(dirname "$0")" && pwd)
if [[ ${1:-} != --inside ]]; then
    staging=$(mktemp -d)
    trap 'rm -rf -- "$staging"' EXIT
    docker build --load --iidfile "$staging/image" -f "$directory/fde-uki.Dockerfile" "$directory"
    image=$(< "$staging/image")
    rm -rf -- "$staging"
    trap - EXIT
    exec docker run --rm --network none \
        --mount "type=bind,source=$directory,target=/tests,readonly" \
        --mount "type=bind,source=$(realpath "$directory/../lib/scripts/fde-build-uki"),target=/fde-build-uki,readonly" \
        --mount "type=bind,source=$(realpath "$directory/../lib/scripts/fde-boot-mode"),target=/fde-boot-mode,readonly" \
        "$image" bash /tests/fde-uki-test.sh --inside
fi

work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
cd "$work"
mkdir inputs outputs extracted wrappers
helper=/fde-build-uki
ukify=/usr/lib/systemd/ukify
measure=/usr/lib/systemd/systemd-measure

# A valid PE stub exercises packaging, not executable Linux boot.
cp /usr/lib/systemd/boot/efi/linux*.efi.stub inputs/linux
printf 'initramfs fixture\000with binary data\n' > inputs/initrd
read -r boot_mode < /fde-boot-mode
printf 'root=LABEL=startos ro quiet BOOT=%s\n' "$boot_mode" > inputs/cmdline
printf 'ID=startos\nNAME=StartOS\nVERSION_ID=0.4\n' > inputs/os-release
openssl req -new -x509 -newkey rsa:2048 -nodes -subj /CN=uki-test \
    -keyout inputs/key -out inputs/cert -days 1 >/dev/null 2>&1
openssl req -new -x509 -newkey rsa:2048 -nodes -subj /CN=other-key \
    -keyout inputs/other-key -out inputs/other-cert -days 1 >/dev/null 2>&1

args=(--linux inputs/linux --initrd inputs/initrd --cmdline inputs/cmdline
    --os-release inputs/os-release --key inputs/key --cert inputs/cert)
fail() { echo "FAIL: $*" >&2; exit 1; }
expect_failure() {
    if "$@" > failure.log 2>&1; then fail "unexpected success: $*"; fi
    if find outputs -name '.fde-uki.*' | grep -q .; then
        fail "failed build left staging files"
    fi
}
expect_absent() {
    [[ ! -e $1 && ! -e $1.pcr11 ]] || fail "failed build published $1"
}

"$ukify" --version
"$measure" --version | head -1
sbverify --version

# An ambient ukify config must not introduce sections into PCR11.
mkdir -p /etc/systemd
printf '[UKI]\nUname=ambient-version\nSplash=/nonexistent-splash\n' > /etc/systemd/ukify.conf
bash "$helper" "${args[@]}" --output outputs/first.efi > stdout.log
[[ ! -s stdout.log ]] || fail 'unexpected stdout protocol'
[[ $(wc -c < outputs/first.efi.pcr11) == 65 ]] || fail 'prediction is not 65 bytes'
LC_ALL=C grep -Ex '[0-9a-f]{64}' outputs/first.efi.pcr11 >/dev/null
sbverify --cert inputs/cert outputs/first.efi
if sbverify --cert inputs/other-cert outputs/first.efi > failure.log 2>&1; then
    fail 'wrong certificate verified UKI'
fi

"$ukify" inspect outputs/first.efi --config=/dev/null --all --json=short > sections.json
for section in linux initrd cmdline osrel sbat; do
    "$ukify" inspect outputs/first.efi --config=/dev/null \
        --section=".$section:binary@extracted/$section" >/dev/null
    jq -e --arg section ".$section" '.[$section].size > 0' sections.json >/dev/null
done
cmp inputs/linux extracted/linux
cmp inputs/initrd extracted/initrd
cmp inputs/cmdline extracted/cmdline
cmp inputs/os-release extracted/osrel
jq -e 'has(".uname") | not' sections.json >/dev/null

measure_args=(--bank=sha256 --json=short --linux=extracted/linux --initrd=extracted/initrd
    --cmdline=extracted/cmdline --osrel=extracted/osrel --sbat=extracted/sbat)
"$measure" calculate --phase= "${measure_args[@]}" | jq -er '.sha256[0].hash' > expected
cmp expected outputs/first.efi.pcr11
"$measure" calculate --phase=enter-initrd "${measure_args[@]}" | jq -er '.sha256[0].hash' > phased
if cmp -s phased expected; then fail 'prediction included enter-initrd phase'; fi

# Changing an authenticated section must invalidate the PE signature.
cp outputs/first.efi tampered.efi
initrd_offset=$(objdump -h tampered.efi | awk '$2 == ".initrd" {print $6}')
[[ -n $initrd_offset ]] || fail 'cannot locate initrd section'
printf X | dd of=tampered.efi bs=1 seek="$((16#$initrd_offset))" conv=notrunc status=none
sbverify --list tampered.efi | grep -q 'uki-test'
if sbverify --cert inputs/cert tampered.efi > failure.log 2>&1; then
    fail 'tampered UKI passed verification'
fi

sleep 1
bash "$helper" "${args[@]}" --output outputs/second.efi >/dev/null
sbverify --cert inputs/cert outputs/second.efi
cmp outputs/first.efi.pcr11 outputs/second.efi.pcr11
sha256sum outputs/first.efi outputs/first.efi.pcr11 > committed.sha256

expect_failure bash "$helper"
expect_failure bash "$helper" --unknown inputs/linux
expect_failure bash "$helper" "${args[@]}" --output
expect_failure bash "$helper" "${args[@]}" --linux inputs/linux --output outputs/duplicate.efi
expect_absent outputs/duplicate.efi
expect_failure bash "$helper" "${args[@]:2}" --linux inputs/missing --output outputs/missing.efi
expect_absent outputs/missing.efi
: > inputs/empty
expect_failure bash "$helper" "${args[@]:2}" --linux inputs/empty --output outputs/empty.efi
expect_absent outputs/empty.efi
expect_failure bash "$helper" "${args[@]}" --output outputs/no-directory/image.efi
expect_failure bash "$helper" "${args[@]}" --output outputs
expect_failure bash "$helper" "${args[@]}" --output outputs/first.efi
printf 'already committed prediction\n' > outputs/companion.efi.pcr11
cp outputs/companion.efi.pcr11 companion.before
expect_failure bash "$helper" "${args[@]}" --output outputs/companion.efi
cmp companion.before outputs/companion.efi.pcr11
[[ ! -e outputs/companion.efi ]] || fail 'overwrote existing companion'
ln -s missing-target outputs/symlink.efi
expect_failure bash "$helper" "${args[@]}" --output outputs/symlink.efi
[[ $(readlink outputs/symlink.efi) == missing-target ]] || fail 'replaced dangling symlink'

cp inputs/cmdline inputs/good-cmdline
for cmdline in 'root=LABEL=startos ro' 'BOOT=ordinary' "BOOT=$boot_mode BOOT=$boot_mode" "BOOT=$boot_mode BOOT=ordinary" "x=\" BOOT=$boot_mode y=\""; do
    printf '%s\n' "$cmdline" > inputs/cmdline
    expect_failure bash "$helper" "${args[@]}" --output outputs/wrong-boot.efi
    expect_absent outputs/wrong-boot.efi
done
for control in '\0' '\r' '\v'; do
    printf "root=LABEL=startos${control} BOOT=%s\n" "$boot_mode" > inputs/cmdline
    expect_failure bash "$helper" "${args[@]}" --output outputs/control-boot.efi
    expect_absent outputs/control-boot.efi
done
mv inputs/good-cmdline inputs/cmdline

cp inputs/key inputs/good-key
printf 'invalid private key\n' > inputs/key
expect_failure bash "$helper" "${args[@]}" --output outputs/signing-failure.efi
expect_absent outputs/signing-failure.efi
mv inputs/good-key inputs/key
cp inputs/key inputs/good-key
openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out inputs/key >/dev/null 2>&1
expect_failure bash "$helper" "${args[@]}" --output outputs/non-rsa.efi
expect_absent outputs/non-rsa.efi
mv inputs/good-key inputs/key
cp inputs/cert inputs/good-cert
cp inputs/other-cert inputs/cert
expect_failure bash "$helper" "${args[@]}" --output outputs/mismatched-cert.efi
expect_absent outputs/mismatched-cert.efi
mv inputs/good-cert inputs/cert

# Fail after publishing the prediction, then after publishing both files.
cat > wrappers/ln <<'EOF'
#!/bin/bash
if [[ ${!#} == "$FAIL_TARGET" ]]; then exit 1; fi
exec /usr/bin/ln "$@"
EOF
chmod +x wrappers/ln
expect_failure env PATH="$work/wrappers:$PATH" FAIL_TARGET="$work/outputs/link-failure.efi" \
    bash "$helper" "${args[@]}" --output outputs/link-failure.efi
expect_absent outputs/link-failure.efi
rm wrappers/ln
cat > wrappers/sync <<'EOF'
#!/bin/bash
if [[ ${!#} == "$FAIL_TARGET" ]]; then exit 1; fi
exec /usr/bin/sync "$@"
EOF
chmod +x wrappers/sync
expect_failure env PATH="$work/wrappers:$PATH" FAIL_TARGET="$work/outputs" \
    bash "$helper" "${args[@]}" --output outputs/sync-failure.efi
expect_absent outputs/sync-failure.efi
sha256sum -c committed.sha256

printf 'PASS: signed UKI sections, tamper rejection, stable pre-phase PCR11, failure isolation\n'
printf 'Fixture: systemd-stub stand-in; executable Linux/UEFI/TPM boot is not covered.\n'
