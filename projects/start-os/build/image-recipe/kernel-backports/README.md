# Signed-kernel backport bridge

This directory owns the forky bridge for generic trixie x86/ARM images, including
nonfree and NVIDIA variants. `stage.sh` adds a build-local archive;
`verify-kernel-backports.sh` checks it after chroot assembly and skips images
without that archive. The parent recipe contains only these two hooks.

`kernel-backports.conf` owns the snapshot, ABI, package version, compiler and
archive-list path. Native APT verifies Debian signatures and package hashes from
the pinned snapshot; historical metadata expiry is disabled for that fixed input.
The signed kernel/module payload archives remain byte-identical. Header compiler
selectors and checksums use trixie's compiler while the ABI and kernel
configuration stay intact. Header directory timestamps are reproducible.

`live-build` binds the flat file archive through its chroot-only archive facility.
`trusted=yes` applies to that generated archive, not the Debian network source.
Neither the source nor its deb files remain in the assembled image. APT selects
needed packages without installing every header into every image.

The image check requires a sufficiently new installed kernel package, identical
module/boot binaries and incoming `GRUB_TOP_LEVEL`. Retained higher-ABI boot files
must not displace the incoming image when returning to official backports.

Read [AGENTS.md](AGENTS.md) for builds, contracts and the removal boundary.
