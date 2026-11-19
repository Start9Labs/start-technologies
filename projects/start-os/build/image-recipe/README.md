# StartOS Image Recipes

`live-build` assembles the StartOS live and installer images inside the Debian
Docker builder defined here. Run the owning recipes from the repository root:

```bash
make start-os-iso PLATFORM=x86_64
make start-os-img PLATFORM=raspberrypi
```

See [StartOS build prerequisites](../../AGENTS.md#prerequisites) for Docker,
architecture emulation and image tooling. `run-local-build.sh` is the recipe's
container wrapper and accepts the prepared StartOS `.deb` path.

Generic x86/ARM images use the isolated
[signed-kernel bridge](kernel-backports/README.md). Its build rules, tests,
configuration and retirement boundary live in that directory.

Raspberry Pi and other vendor kernels, and RISC-V kernel selection, are unchanged.
Both Pi `rpi-v8` and `rpi-2712` packages must remain available. The root
[kernel-security review requirements](../../../../AGENTS.md#releases) govern
release validation.
