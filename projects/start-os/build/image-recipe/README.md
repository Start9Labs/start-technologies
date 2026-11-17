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

Generic images use Debian-signed backports kernels. Raspberry Pi uses versioned
vendor packages; both its `rpi-v8` and `rpi-2712` kernels must remain available.
The [kernel security release assessment](../../../../AGENTS.md#releases) governs
updates; NVIDIA compatibility must not freeze a vulnerable kernel.
