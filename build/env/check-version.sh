#!/bin/bash

cd "$(dirname "${BASH_SOURCE[0]}")"

# Materializes the OS image's /usr/lib/startos/VERSION.txt. version.sh owns where each
# project's version comes from.
VERSION="$(./version.sh startos)"

if ! [ -f ./VERSION.txt ] || [ "$(cat ./VERSION.txt)" != "$VERSION" ]; then
    echo -n "$VERSION" > ./VERSION.txt
fi

echo -n ./build/env/VERSION.txt