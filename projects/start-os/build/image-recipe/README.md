# StartOS Image Recipes

Code that uses `live-build` to create the StartOS live and installer
images.

Raspberry Pi image builds reject pins outside the 6.18 `+rpt` series or below
6.18.56, the CVE-2026-98432 fixed release. The 6.18.33 pin therefore blocks Pi
image creation until supported fixed packages for both `rpi-v8` and `rpi-2712`
are available and the pin is updated. The guard prevents vulnerable builds; it
does not update existing images or devices. A different kernel series requires
reviewing the guard against that series' vendor fix status.

If you want to build a local image in the exact same environment used to build
official StartOS images, you can use the `run-local-build.sh` helper script:

```bash
# Prerequisites
sudo apt-get install -y debspawn binfmt-support
sudo mkdir -p /etc/debspawn/ && echo "AllowUnsafePermissions=true" | sudo tee /etc/debspawn/global.toml

# Build image
./run-local-build.sh
```

In order for the build to work properly, you will need debspawn >= 0.5.1, the
build may fail with prior versions.
