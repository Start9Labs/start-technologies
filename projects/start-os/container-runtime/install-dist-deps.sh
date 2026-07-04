#!/bin/bash

cd "$(dirname "${BASH_SOURCE[0]}")"

set -e

cat ./package.json | sed 's/file:\.\([.\/]\)/file:..\/.\1/g' > ./dist/package.json

# --install-links materializes the file: deps' transitive tree (e.g. zod-deep-partial) as real files, not symlinks
npm --prefix dist install --install-links --omit=dev