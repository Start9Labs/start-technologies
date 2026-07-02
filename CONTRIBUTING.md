# Contributing

This guide is for contributing to the Start9 monorepo (StartOS and the other products that live here). If you are interested in packaging a service for StartOS, visit the [packaging guide](https://docs.start9.com/packaging). If you are interested in promoting, providing technical support, creating tutorials, or helping in other ways, please visit the [Start9 website](https://start9.com/contribute).

This file covers what is **common to the whole monorepo** — the shared toolchain, branch policy, the cross-cutting test/format entry points, and code/commit conventions. **Per-product system dependencies, build targets, and deploy steps live in each product's own `CONTRIBUTING.md`** (e.g. [`projects/start-os/CONTRIBUTING.md`](projects/start-os/CONTRIBUTING.md) for building the StartOS OS image).

## Documentation

The repo root's docs split across four files:

- `README.md` — what this is
- `ARCHITECTURE.md` — how it's built (the monorepo layout)
- `CONTRIBUTING.md` — this file; how to contribute
- `AGENTS.md` — AI-developer/agent operating rules (`CLAUDE.md` is a one-line `@AGENTS.md` import)

**These docs must be kept up to date.** When you change project structure, conventions, build process, or product context, update the relevant file(s) in the same change — do not defer. Each product and shared library keeps its own `README.md`/`ARCHITECTURE.md`/`CONTRIBUTING.md`/`AGENTS.md` for what is specific to it — see `projects/*/`, `shared-libs/crates/start-core/`, `shared-libs/ts-modules/`, and `projects/start-os/container-runtime/`.

## Collaboration

- [Matrix](https://matrix.to/#/#dev-startos:matrix.start9labs.com)
- Security issues: [security@start9.com](mailto:security@start9.com)

## Environment Setup

> Debian/Ubuntu is the only officially supported build environment.
> MacOS has limited build capabilities and Windows requires [WSL2](https://learn.microsoft.com/en-us/windows/wsl/install).

The shared toolchain below is enough to build the Rust bins and the web apps. **Individual products need more** — most notably the StartOS OS image, which adds multi-arch emulation and image-packaging tooling. See each product's `CONTRIBUTING.md` for its additional system dependencies.

**Web-UI work skips most of this.** The Angular front ends build and run standalone against mock data — they need only Node 24 and Make, no Rust, Docker, or OS-image tooling. See [`shared-libs/ts-modules/CONTRIBUTING.md`](shared-libs/ts-modules/CONTRIBUTING.md).

```sh
# Common build tooling
sudo apt update
sudo apt install -y ca-certificates curl gpg build-essential git \
  sed grep gawk jq gzip brotli rsync

# Container backend (Docker) — used by .s9pk packaging and OS image builds
curl -fsSL https://download.docker.com/linux/debian/gpg | sudo gpg --dearmor -o /usr/share/keyrings/docker-archive-keyring.gpg
echo "deb [arch=$(dpkg-architecture -q DEB_HOST_ARCH) signed-by=/usr/share/keyrings/docker-archive-keyring.gpg] https://download.docker.com/linux/debian bookworm stable" | sudo tee /etc/apt/sources.list.d/docker.list
sudo apt update
sudo apt install -y containerd.io docker-ce docker-ce-cli docker-compose-plugin
sudo usermod -aG docker $USER
sudo su $USER

# Rust (the nightly toolchain is used for formatting)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh # proceed with default installation

# Node.js 24 (required by Angular 22's CLI)
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/master/install.sh | bash
source ~/.bashrc
nvm install 24
nvm use 24
nvm alias default 24 # this prevents your machine from reverting back to another version
```

### Cloning

```sh
git clone https://github.com/Start9Labs/start-technologies.git
cd start-technologies
```

The repo has four integration branches. `master` is for the current release — if the latest release is a pre-release (i.e. "beta.X"), PRs for new features and bugfixes go here. Otherwise target a `next/` branch for the release the change should ship in: `next/patch`, `next/minor`, or `next/major`. If you are unsure which to target, ask a maintainer.

## Building

This is a monorepo: one root Cargo workspace and one Angular workspace, both rooted at the repo root. The root `Makefile` is a thin orchestrator (it `include`s each product's `build.mk`) — run `make` with no target to print a help summary; there is no default target. Run build commands from the repo root.

- **A single Rust bin:** `cargo build -p <crate> --bin <bin>` — crates are `start-os` (`startbox` / `start-container`), `start-cli`, `start-registry` (`registrybox`), `start-tunnel` (`tunnelbox`), and `startwrt-core` (`startwrt`).
- **A whole product** (bins + UI + packaging) has its own `make` targets and build instructions in its `CONTRIBUTING.md`:

| Product | Primary build target | Build & deploy docs |
| --- | --- | --- |
| StartOS (OS image, UIs, device deploy) | `make startos` | [`projects/start-os/CONTRIBUTING.md`](projects/start-os/CONTRIBUTING.md) |
| start-cli | `make cli` | [`projects/start-cli/CONTRIBUTING.md`](projects/start-cli/CONTRIBUTING.md) |
| start-registry | `make registry` | [`projects/start-registry/CONTRIBUTING.md`](projects/start-registry/CONTRIBUTING.md) |
| StartTunnel | `make tunnel` | [`projects/start-tunnel/CONTRIBUTING.md`](projects/start-tunnel/CONTRIBUTING.md) |
| StartWRT | `make startwrt` (`make startwrt-image` for the full OpenWrt image — hours, needs the submodule) | [`projects/start-wrt/CONTRIBUTING.md`](projects/start-wrt/CONTRIBUTING.md) |
| Start SDK | `make bundle` (from `projects/start-sdk`) | [`projects/start-sdk/CONTRIBUTING.md`](projects/start-sdk/CONTRIBUTING.md) |
| Web (shared libs + app UIs) | `npm run build:ui` | [`shared-libs/ts-modules/CONTRIBUTING.md`](shared-libs/ts-modules/CONTRIBUTING.md) |

`make ts-bindings` regenerates the TypeScript bindings from the Rust types, and `make clean` removes all compiled artifacts. Cross-layer changes (Rust → bindings → SDK → web/runtime) are described in [ARCHITECTURE.md](ARCHITECTURE.md#build-pipeline).

### Build configuration

Builds are parameterized by environment variables shared across all products:

| Variable             | Description                                                                                       |
| -------------------- | ------------------------------------------------------------------------------------------------ |
| `PLATFORM`           | Target platform (e.g. `x86_64`, `aarch64`, `riscv64`). For non-OS products it only derives `ARCH`. |
| `ENVIRONMENT`        | Hyphen-separated feature flags; the available options depend on the product.                      |
| `PROFILE`            | Build profile: `release` (default) or `dev`.                                                      |
| `GIT_BRANCH_AS_HASH` | Set to `1` to use the git branch name as the version hash (avoids rebuilds).                      |

Each product's `CONTRIBUTING.md` documents the `PLATFORM` values and `ENVIRONMENT` flags it actually supports.

## Testing

```bash
make test                    # all tests
make test-core               # Rust (shared-libs/crates/start-core)
make test-sdk                # SDK
make test-container-runtime  # container runtime
make test-startwrt           # StartWRT Rust crates

# Run a specific Rust test
cd shared-libs/crates/start-core && cargo test <test_name> --features=test
```

Each product's `CONTRIBUTING.md` covers its own scoped tests.

## Formatting

```bash
make format          # format every project
make format-check    # read-only check (what CI runs)
```

Or scope it to one project — each has a `format-check-*` read-only variant that CI runs:

```bash
make format-core         # shared Rust crates
make format-cli          # start-cli  (also format-registry / format-tunnel / format-startos / format-startwrt)
make format-web          # the Angular workspace (shared libs + all app UIs, incl. brochure)
make format-sdk          # the SDK
```

Run the formatters before committing. Configuration is handled by `rustfmt.toml` (Rust) and prettier configs (TypeScript).

## Code Style Guidelines

### Documentation & Comments

**Rust:**

- Add doc comments (`///`) to public APIs, structs, and non-obvious functions
- Use `//` comments sparingly for complex logic that isn't self-evident
- Comments should be shorthand, not prose. Most comments can say what they need to in a single line.

**TypeScript:**

- Document exported functions and complex types with JSDoc
- Keep comments focused on "why" rather than "what"

**General:**

- Don't add comments that just restate the code
- Update or remove comments when code changes
- TODOs should include context: `// TODO(username): reason`

## Commits / PRs

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

**Types:**

- `feat` - New feature
- `fix` - Bug fix
- `docs` - Documentation only
- `style` - Formatting, no code change
- `refactor` - Code change that neither fixes a bug nor adds a feature
- `test` - Adding or updating tests
- `chore` - Build process, dependencies, etc.

**Examples:**

```
feat(web): add dark mode toggle
fix(core): resolve race condition in service startup
docs: update CONTRIBUTING.md with style guidelines
refactor(sdk): simplify package validation logic
```
