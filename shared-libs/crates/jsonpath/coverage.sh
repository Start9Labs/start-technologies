#!/usr/bin/env bash

#
# cargo install cargo-tarpaulin
#

set -e

cargo tarpaulin --exclude-files parser/mod.rs -v --all