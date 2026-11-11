#!/bin/bash

set -e

cd "$(dirname "${BASH_SOURCE[0]}")"

IFS="-" read -ra FEATURES <<< "$ENVIRONMENT"
FEATURES+=("${ARCH}")
case "$ARCH" in
    x86_64|aarch64) FEATURES+=("uefi") ;;
esac
if [ "$ARCH" != "$PLATFORM" ]; then
    FEATURES+=("${PLATFORM}")
fi
if [[ "$PLATFORM" =~ -nonfree$ ]]; then
    FEATURES+=("nonfree")
fi
if [[ "$PLATFORM" =~ -nvidia$ ]]; then
    FEATURES+=("nonfree")
    FEATURES+=("nvidia")
fi

feature_file_checker='
/^#/ { next }
/^\+ [a-z0-9.-]+$/ { next }
/^- [a-z0-9.-]+$/ { next }
{ exit 1 }
'

for type in conflicts depends; do
    pkgs=()
    for feature in ${FEATURES[@]}; do
        file="$feature.$type"
        if [ -f $file ]; then
            # TODO check for syntax errrors
            cat $file | awk "$feature_file_checker"
            for pkg in $(cat $file | awk '/^\+/ {print $2}'); do
                pkgs+=($pkg)
            done
        fi
    done
    for pkg in $(cat $type); do
        SKIP=
        for feature in ${FEATURES[@]}; do
            file="$feature.$type"
            if [ -f $file ]; then
                if grep "^- $pkg$" $file > /dev/null; then
                    SKIP=yes
                fi
            fi
        done
        if [ -z $SKIP ]; then
            pkgs+=($pkg)
        fi
    done
    (IFS=$'\n'; echo "${pkgs[*]}") | sort -u > ../lib/$type
done
