# Environment Setup

Before building service packages, you need to install several development tools on your workstation. This page lists each prerequisite and how to install it. The final section — [Set Up Your Packaging Workspace](#set-up-your-packaging-workspace) — scaffolds the AI-assisted workspace that all packaging is designed around.

> [!NOTE]
> The **Linux** examples below install packages with `apt`, for Debian-based distros (Debian, Ubuntu, Mint, PopOS, …). On another distro, use your package manager to install the same packages.

## StartOS Device

You must have a computer running StartOS to test your packages. Follow the [installation guide](/start-os/installing-startos.html) to install StartOS on a physical device or VM.

## Docker

[Docker](https://docs.docker.com/get-docker/) is essential for building and managing container images that will be used for the final `.s9pk` build. It handles pulling base images and building custom container images from Dockerfiles.

Follow the [official Docker installation guide](https://docs.docker.com/engine/install/) for your platform.

Docker must be **running** when you build a package, and your user must be able to use it:

{{#tabs global="platform"}}

{{#tab name="Linux"}}

The daemon runs as a service — start it with `sudo systemctl start docker`. By default only root can talk to it, so add your user to the `docker` group once (then **log out and back in**), otherwise every build fails with `permission denied ... /var/run/docker.sock`:

```sh
sudo usermod -aG docker $USER
```

{{#endtab}}

{{#tab name="Mac"}}

Start Docker Desktop; it runs the daemon for you. (Docker Desktop covers Windows too.)

{{#endtab}}

{{#endtabs}}

> [!TIP]
> Confirm it works with `docker run --rm hello-world` before continuing.

## Make

[Make](https://www.gnu.org/software/make/) is a build automation tool used to execute build scripts defined in Makefiles and coordinate the packaging workflow (building and installing s9pk binaries to StartOS).

{{#tabs global="platform"}}

{{#tab name="Linux"}}

```sh
sudo apt install build-essential
```

{{#endtab}}

{{#tab name="Mac"}}

```sh
xcode-select --install
```

{{#endtab}}

{{#endtabs}}

## Node.js v22 (Latest LTS)

[Node.js](https://nodejs.org/en/) is required for compiling TypeScript code used in StartOS package configurations.

The recommended installation method is [nvm](https://github.com/nvm-sh/nvm). If you don't already have `nvm`, install it, then **close and reopen your terminal** (or `source ~/.bashrc` / `source ~/.zshrc`) so the `nvm` command is available:

```sh
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.1/install.sh | bash
```

Then install and select Node.js v22:

```sh
nvm install 22
nvm use 22
```

Alternatively, download Node.js v22 (or newer) directly from [nodejs.org](https://nodejs.org/) — make sure `node --version` reports v22+ afterward.

## SquashFS

SquashFS is used to create compressed filesystem images that package your compiled service code.

{{#tabs global="platform"}}

{{#tab name="Linux"}}

```sh
sudo apt install squashfs-tools squashfs-tools-ng
```

{{#endtab}}

{{#tab name="Mac"}}

Requires [Homebrew](https://brew.sh/):

```sh
brew install squashfs
```

{{#endtab}}

{{#endtabs}}

## cURL

[cURL](https://curl.se/) downloads the `start-cli` installer script in the next step. It is pre-installed on macOS and most Linux systems; install it if missing.

{{#tabs global="platform"}}

{{#tab name="Linux"}}

```sh
sudo apt install curl
```

{{#endtab}}

{{#tab name="Mac"}}

Already included.

{{#endtab}}

{{#endtabs}}

## Start CLI

[start-cli](https://github.com/Start9Labs/start-technologies) is the core development toolkit for building StartOS packages. It provides package validation, s9pk file creation, and development workflow management.

Install using the automated installer script:

```sh
curl -fsSL https://start9.com/start-cli/install.sh | sh
```

## Git

[Git](https://git-scm.com/) is used by `start-cli s9pk init-workspace` to fetch the Start9 monorepo — the packaging guide, and the SDK and OS source behind it — and to keep it up to date afterward.

{{#tabs global="platform"}}

{{#tab name="Linux"}}

```sh
sudo apt install git
```

{{#endtab}}

{{#tab name="Mac"}}

Installed with the Command Line Tools (`xcode-select --install`, above), or `brew install git`.

{{#endtab}}

{{#endtabs}}

## jq

The build uses [jq](https://jqlang.github.io/jq/) to read your package's manifest and print the build summary, so it must be installed.

{{#tabs global="platform"}}

{{#tab name="Linux"}}

```sh
sudo apt install jq
```

{{#endtab}}

{{#tab name="Mac"}}

```sh
brew install jq
```

{{#endtab}}

{{#endtabs}}

## Verification

After installation, verify all tools are available:

```sh
docker --version
docker run --rm hello-world   # confirms the daemon is running and you have access
make --version
node --version                # must be v22 or newer
npm --version
mksquashfs -version
git --version
curl --version
jq --version
start-cli --version
```

> [!TIP]
> If any command is not found, revisit the installation steps for that tool and ensure it is on your system PATH. If `docker run --rm hello-world` fails, re-read the Docker note above (the daemon must be running, and on Linux your user must be in the `docker` group).

## Set Up Your Packaging Workspace

StartOS packaging is designed to be done with an AI coding agent. `start-cli` scaffolds an AI-ready **packaging workspace** in one command — a directory that holds the packaging guide and an agent-context file, so any assistant you open there already knows how to build a StartOS package. If you use [Claude Code](https://docs.anthropic.com/en/docs/claude-code), Start9 recommends the Opus 4.7 or later model.

### Create the workspace

```sh
start-cli s9pk init-workspace start9-workspace
cd start9-workspace
```

This clones the Start9 monorepo into `start-technologies/`, sets up the agent-context files (`AGENTS.md`, your own `AGENTS.local.md`, and a `CLAUDE.md` that loads both), and creates a `.startos/` directory that marks the workspace and holds your package-signing key and host/registry config:

```
start9-workspace/
├── .startos/              ← workspace marker: build-key (signs your packages) + config.yaml (hosts, registries)
├── AGENTS.md              ← agent context (symlink to the guide's Agent Context page), read by AI assistants
├── AGENTS.local.md        ← your own notes, kept across guide updates
├── CLAUDE.md              ← loads AGENTS.md + AGENTS.local.md (Claude Code)
└── start-technologies/    ← the monorepo: the guide, the SDK source, the OS source
```

You get the **whole** monorepo, not just the guide. That's deliberate: when the guide can't settle a question, the SDK source (`projects/start-sdk/lib`) and the StartOS source (`projects/start-os`, `shared-libs/`) are right there to read — and if you find a bug, you're already in a repo you can open a pull request from. The clone is `--filter=blob:none`, so file contents are fetched on demand: it lands in a few seconds and takes ~75 MB, while `git log`, `git blame`, and rebase all behave normally.

The context lives once, at the workspace root — it is never copied into your package repos. Open the workspace in your AI tool and it picks up `AGENTS.md` / `CLAUDE.md` automatically. You can read exactly what it contains on the [Agent Context](./agent-context.md) page.

### Already have the monorepo?

If you already keep a `start-technologies` checkout — you work on StartOS itself, or you've cloned it for another reason — don't let the workspace clone a second copy. Point at the one you have **before** running `init-workspace`, and it will use it:

```sh
mkdir start9-workspace && cd start9-workspace
ln -s /path/to/your/start-technologies start-technologies
start-cli s9pk init-workspace .
```

`init-workspace` skips the clone whenever `start-technologies` already resolves to a directory, so the symlink is left alone and everything else is provisioned around it. The workspace `AGENTS.md` links through it, and `s9pk init-package` scaffolds from its package template, exactly as with a fresh clone.

> [!IMPORTANT]
> A symlinked checkout is **yours to maintain**. Skip the `git pull` in [Keep it current](#keep-it-current): that repo has its own branches and its own work in progress, and a blind pull would fast-forward whatever branch happens to be checked out rather than refresh the guide. Update it on your own schedule instead.

### Nested workspaces and config resolution

Workspaces can be nested — running `init-workspace` inside another workspace is fine. When `start-cli` needs a workspace's signing key or targets (building, signing, reading `host`/`registry`), it walks **up** from the current directory and uses the nearest `.startos/`. So an inner workspace transparently overrides an outer one, and settings you don't override are inherited from above — conceptually a deep merge of every `.startos/` on the path, innermost first.

The one thing `init-workspace` refuses is running **inside a package repo**: a workspace is the directory that _holds_ package repos, not a package itself. If you already have package repos, run `init-workspace` in the directory that contains them (their parent); building, signing, and publishing then walk up to find the workspace. Starting fresh, run it in a new directory, then `start-cli s9pk init-package` inside it.

Until a workspace exists, `make` / `s9pk pack` / `s9pk publish` fail with a message pointing you to `init-workspace` — packaging is designed around the workspace (and its AI guide), so there is no build-key to sign with until you create one.

> [!NOTE]
> There's no automatic migration from an older global `~/.startos`. To reuse a previous signing key, copy it into a workspace yourself: `cp ~/.startos/developer.key.pem <workspace>/.startos/build-key`.

### Hosts and registries

The `.startos/config.yaml` created with the workspace defines named **host** targets (your StartOS boxes) and **registry** targets:

```yaml
schema: 1
host:
  default: https://dev-vm.local
  prod: https://prodbox.local
registry:
  default: https://alpha-registry-x.start9.com
  beta: https://beta-registry.start9.com
  prod: https://registry.start9.com
```

The `registry` entries are Start9's, pre-filled — you only need them if you plan to **publish** a package, so you can ignore them while testing locally. The `host` entries are the StartOS devices you install to; edit `host.default` to point at your own box.

Your device's address is shown in the StartOS web interface (it looks like `https://adjective-noun.local`, or use its IP such as `https://192.168.1.100`). Set it as `host.default`, for example:

```yaml
host:
  default: https://adjective-noun.local
```

> [!TIP]
> Setting `host.default` lets you install with `make install` — the recommended way to work on a package, since it builds and pushes to your device in one repeatable command. It also requires logging in once with `start-cli auth login` (it prompts for your StartOS master password). If you'd rather not set up the CLI yet, you can sideload the `.s9pk` through the web interface instead — see [Quick Start](./quick-start.md#install-to-startos).

Any `start-cli` command takes `-H`/`--host` and `-r`/`--registry`. Pass a **profile name** to use one of these entries, or a **URL** to target something directly:

```sh
start-cli -H prod <command>                  # uses host.prod
start-cli -r beta <command>                  # uses registry.beta
start-cli -H https://my-box.local <command>  # a URL works too
```

With no flag, the `default` entry is used. `start-cli` finds this config by walking up from the current directory, so it works anywhere inside the workspace.

> [!NOTE]
> As of `@start9labs/start-sdk` 2.0, `make install` and `make publish` resolve their target through `start-cli` — the workspace `.startos/config.yaml` profiles, or `-H` / `-r`. (Older `s9pk.mk` parsed a single `host:` / `registry:` URL from the global `~/.startos/config.yaml`.) See [Makefile](./makefile.md).

### Keep it current

The guide, the package template, and the agent context all live in `start-technologies/`, so syncing it refreshes everything at once. Pull it at the start of each session:

```sh
git -C start-technologies pull --ff-only
```

There's no separate update command — re-running `init-workspace` on an existing workspace just fills in anything missing, and your `AGENTS.local.md` is never touched.

Your environment is ready. Continue to [Quick Start](./quick-start.md) to scaffold and build your first package inside the workspace.
