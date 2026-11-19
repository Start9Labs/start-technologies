# Kernel bridge maintenance

This directory is the complete temporary kernel backport implementation, including
its configuration, tests and Make rules. The parent image recipe may stage and
verify the bridge through its two hooks; keep package/snapshot/platform policy
inside this directory.

Build with `make start-os-kernel-backports KERNEL_DEB_ARCH=amd64` or `arm64`.
Outputs are `results/kernel-backports/<arch>/<version>/`. Run
`make start-os-kernel-backports-test` for focused coverage. Both targets use the
existing product build infrastructure.

Signed kernel/module payload bytes and Debian signatures must survive
repackaging. Header compiler selectors may change with their package checksums;
the ABI and kernel configuration stay intact. Rebuilding or locally signing the
kernel is outside this bridge's contract.

## Retirement

Retire the bridge once official trixie-backports provides a patched signed kernel
and usable matching headers/compiler dependencies. Its version sorts below the
corresponding official backport. Never mix its forky ABI with official backports
modules or headers.

The removal boundary is:

- Delete this directory.
- Delete the two `kernel-backports/` calls in the parent `build.sh`.
- Delete this directory's Make include from `projects/start-os/build.mk`.
- Delete the `Archive kernel backport packages` step in `startos-image.yaml`.
- Remove the bridge's explicit `binutils` dependency from the image Dockerfile.
- Replace the bridge pointers in the parent README and StartOS AGENTS with normal
  backports selection documentation.

Keep the permanent family-wide backports preferences, target-header-based kbuild
lookup, their tests/Make target, and the image-built `GRUB_TOP_LEVEL` contract.
Retain the security changelog benefit, updating its kernel version if necessary.
Preserve incoming-image boot preference over retained higher-ABI kernels when
verifying the official-backport transition.
