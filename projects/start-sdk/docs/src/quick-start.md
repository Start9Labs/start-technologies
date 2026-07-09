# Quick Start

This guide walks you through scaffolding a new service package, building it, and installing it on StartOS. The scaffold is a working **Hello World** service — your starting point for packaging any app.

> [!NOTE]
> Complete [Environment Setup](./environment-setup.md) first — including [creating your packaging workspace](./environment-setup.md#set-up-your-packaging-workspace). `start-cli s9pk init-package` only runs inside a workspace.

## Scaffold the Package

From the root of the workspace you created during Environment Setup, scaffold a new package:

```sh
start-cli s9pk init-package "Hello World"
```

`init-package` normalizes the display name to a package ID, creates `hello-world-startos/` from the bundled template — a barebones, buildable Hello World clone — and runs `npm install` for you. It leaves a `TODO.md` checklist that takes the package from clone to release-ready.

Your workspace now looks like:

```
start9-workspace/
├── .startos/
├── AGENTS.md
├── AGENTS.local.md
├── CLAUDE.md
├── start-technologies/
└── hello-world-startos/    ← your new package
```

> [!TIP]
> Already have a package repo? Clone it into the workspace alongside `start-technologies/` and build it the same way.

Make sure Docker is running first (`docker ps` should succeed — see [Environment Setup](./environment-setup.md#docker)), then build for **your StartOS device's architecture** — use `x86` for a typical Intel/AMD server or VM, or `arm` for a Raspberry Pi or other ARM board:

```sh
cd hello-world-startos
make x86        # or: make arm
```

Dependencies were already installed by `init-package`, so this goes straight to building. The first build pulls the service's container image, so it can take a few minutes. Building a single architecture is the fast path for development; it produces `hello-world_x86_64.s9pk` (or `hello-world_aarch64.s9pk`). Building every architecture (`make`) or one multi-arch package (`make universal`) is slower and only needed when you publish to a registry — see [Makefile](./makefile.md) for all build targets.

## Install to StartOS

You need a device running StartOS (from [Environment Setup](./environment-setup.md#startos-device)) on the same network.

### Recommended: `make install` from the command line

This is the way to work on a package: build and push to your device in a single command, repeated on every change. Set it up once:

1. Point your workspace at the device — set `host.default` in `.startos/config.yaml` to your device's address (see [Hosts and registries](./environment-setup.md#hosts-and-registries)).
2. Log in — `start-cli auth login` (enter your StartOS master password).

Then build and install for your device's architecture in one step, from the package directory:

```sh
make x86 install        # or: make arm install
```

Every later change is just another `make x86 install`. See [Makefile — Installation](./makefile.md#installation) for details (including the one-time certificate trust `make install` needs).

### Alternative: sideload via the web interface

No command-line setup — a good way to get your first `.s9pk` onto a device, or if you haven't configured the CLI yet:

1. Open your StartOS device in a browser and log in.
2. Click **Sideload** in the top navigation bar.
3. Select the `.s9pk` you just built (`hello-world_x86_64.s9pk` or `hello-world_aarch64.s9pk`).

See [Sideloading](/start-os/sideloading.html) for details.

## Next Steps

With Hello World running on your server, you're ready to package your own service. Open `hello-world-startos/` in your AI assistant and point it at the `TODO.md` checklist — it takes the package from Hello World clone to a real service (descriptions, image, icon, interfaces, daemons, docs).

Then browse the [Recipes](./recipes.md) to find the patterns your service needs — each describes an approach and points you to reference docs and real package code.
