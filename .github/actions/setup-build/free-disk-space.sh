#!/bin/bash
# Purges preinstalled toolchains the builds never use. Runners that already have
# FREE_DISK_SPACE_MIN_GB available skip it.
set -u

MIN_GB=${FREE_DISK_SPACE_MIN_GB:-50}
avail=$(df --output=avail -BG / | tail -n 1 | tr -dc '0-9')
df -h /
if [ "${avail:-0}" -ge "$MIN_GB" ]; then
  echo "${avail}G available; skipping cleanup."
  exit 0
fi

sudo apt-get remove --purge -y azure-cli || true
sudo apt-get remove --purge -y firefox || true
sudo apt-get remove --purge -y ghc-* || true
sudo apt-get remove --purge -y google-cloud-sdk || true
sudo apt-get remove --purge -y google-chrome-stable || true
sudo apt-get remove --purge -y powershell || true
sudo apt-get remove --purge -y php* || true
sudo apt-get remove --purge -y ruby* || true
sudo apt-get remove --purge -y mono-* || true
sudo apt-get autoremove -y
sudo apt-get clean
sudo rm -rf /usr/lib/jvm
sudo rm -rf /usr/local/.ghcup
sudo rm -rf /usr/local/lib/android
sudo rm -rf /usr/share/dotnet
sudo rm -rf /usr/share/swift
if [ -n "${AGENT_TOOLSDIRECTORY:-}" ]; then
  sudo rm -rf "$AGENT_TOOLSDIRECTORY"
fi
df -h /
