# Workspace and prerequisites

Only needed when Phase 0's check finds no workspace, or a prerequisite is missing.
`environment-setup.md` in the guide owns the per-platform install matrix — point at it,
don't restate it here.

## Consent first

Two actions in this file need the user's agreement before you take them, every time:

- **Creating a workspace.** It writes `AGENTS.md`, `AGENTS.local.md`, `CLAUDE.md`,
  `.claude/`, `.agents/` and `.startos/` into a directory the user picked for something
  else, clones ~75 MB, and generates a signing key that becomes that workspace's
  permanent identity. Ask, and say where you propose to put it.
- **Installing anything.** Report what is missing and the command that fixes it; let the
  user run it.

**Never run `sudo`.** Every prerequisite install needs it, and it is the user's package
manager and the user's call. Hand over the exact command and wait.

## What counts as a workspace

A `.startos/` directory holding `build.key.pem` **or** a `schema:`-tagged `config.yaml`.
`start-cli` walks up from the current directory and uses the nearest one, so:

- **Nesting is allowed.** An inner workspace transparently overrides an outer one, and
  anything it doesn't set is inherited from above.
- **A bare `.startos/` is not a marker**, and neither is the legacy global `~/.startos`.
  A directory with one of those is not a workspace and will be walked straight past.
- **`init-workspace` refuses to run inside a package repo** — it detects a `package.json`
  depending on the SDK, or a scaffolded `startos/` layout. A workspace is the directory
  that _holds_ package repos. If the user already has some, run it in their parent.

## Prerequisites

Check all of them at once; report the whole set that is missing, not one per turn.

```bash
docker --version && docker run --rm hello-world >/dev/null && echo "docker ok"
make --version | head -1
node --version          # must be v22+
npm --version
mksquashfs -version | head -1
git --version
curl --version | head -1
jq --version
start-cli --version
```

| Missing         | Debian/Ubuntu                                                                                                                     |
| --------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| docker          | the [official install guide](https://docs.docker.com/engine/install/), then `sudo usermod -aG docker $USER` and **a full logout** |
| make            | `sudo apt install build-essential`                                                                                                |
| node v22+       | `nvm install 22 && nvm use 22`, or a v22+ build from nodejs.org                                                                   |
| mksquashfs      | `sudo apt install squashfs-tools squashfs-tools-ng`                                                                               |
| git · curl · jq | `sudo apt install git curl jq`                                                                                                    |
| start-cli       | `curl -fsSL https://start9.com/start-cli/install.sh \| sh`                                                                        |

On macOS, and for the full explanation of each, send the user to `environment-setup.md`.

Two failures that look like something else:

- **`permission denied … /var/run/docker.sock`** means the `docker` group change hasn't
  taken effect. It needs a real logout, not a new shell.
- **Node below v22** compiles the package until it doesn't, usually somewhere unhelpful.
  Check the version rather than trusting that `node` exists.

## Creating the workspace

Once the user agrees, and from a directory that is **not** a package repo:

```bash
start-cli s9pk init-workspace <path>     # path optional; defaults to the current directory
```

It clones the monorepo into `start-technologies/` on `live-docs`, symlinks `AGENTS.md`
at the guide's Agent Context page, writes an `AGENTS.local.md` stub and a `CLAUDE.md`
that loads both, links the fleet's skills at `.claude/skills` and `.agents/skills`, and
provisions `.startos/` with `config.yaml` and a freshly generated `build.key.pem`.

**It is idempotent** — a re-run fills in only what is missing, never overwrites
`AGENTS.local.md`, and never regenerates the build key. So a workspace that is missing a
piece is repaired by running it again, not by hand-writing the file.

**Let the workspace clone its own checkout, even when the user already keeps one.** A
development checkout sits on `master` and moves with its branches; the workspace's sits
on `live-docs`, the branch that describes the SDK `npm install` can actually resolve.
`environment-setup.md` § Already have the monorepo? says the same, and why.

## After it exists

Neither of these is optional if Phase 4 is going to run:

1. **Set `host.default`** in `.startos/config.yaml`. It ships commented out because no
   address suits everyone. Take the address from the StartOS web interface — the `.local`
   name is built from a hostname the user can change — or use the box's IP.
2. **`start-cli auth login`** once, against that host. It prompts for the StartOS master
   password, so it is the user's to run, not yours.

`init-package` runs `npm install` in the new package itself, so there is no separate
dependency step at the workspace level.
