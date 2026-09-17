# Upstream research

What to establish before the gate, and how to establish it. Every line is a fact to
**verify with a tool**, not to recall. Where a fact turns out to be unknowable, say so
at the gate — an admitted gap is a decision the user can make; a guess is a bug you ship.

Work top to bottom. Sections 1–3 can disqualify the package outright, so do them first.

---

## 1 · Identity, license and health

- Canonical upstream repo, project homepage, and docs site. Watch for renamed or forked
  projects and abandoned mirrors that outrank the real one in search.
- **License**, and whether it permits redistribution as part of a package. Note any
  source-available / BSL / "free for personal use" terms, a paid tier, an enterprise
  edition, or a license key that gates core function.
- **Trademark / branding terms** — some projects forbid distributing under their name.
- Project health: last release, release cadence, open-issue trend, number of active
  maintainers, whether self-hosting is officially supported or merely tolerated.

```bash
gh api repos/<owner>/<repo> --jq '{stars:.stargazers_count,license:.license.spdx_id,archived,pushed_at}'
gh api repos/<owner>/<repo>/releases --jq '.[0:5][] | "\(.tag_name)  \(.published_at)"'
```

## 2 · Distribution — the image

The single most load-bearing decision. Read `recipe-prebuilt-image.md` before pinning
anything.

- **Which images exist**: official upstream, `linuxserver/*`, `ghcr.io/<org>/*`, other
  community builds. Prefer the one upstream itself publishes and documents.
- **The repository exists and the tag is published.** Never write a `dockerTag` you have
  not seen in a tag listing.
- **It ships `x86_64` _and_ `aarch64`.** An amd64-only image is a blocker — say so at the
  gate along with whether a `Dockerfile` build is practical.
- **Image config**: entrypoint and cmd, `USER`, declared `VOLUME`s, `EXPOSE`d ports,
  `HEALTHCHECK`, labels, base distro, size.
- **Init system**: `s6-overlay` (every `linuxserver/*` image), `tini`, `dumb-init`,
  `supervisord`. Any of these means `runAsInit: true`.
- **How the image is versioned** — `latest`, semver, `v`-prefixed, date tags, digest-only
  — and where you will read the next version from. That answer becomes `UPDATING.md`.

```bash
# tags (Docker Hub)
curl -s "https://hub.docker.com/v2/repositories/<org>/<name>/tags?page_size=50" | jq -r '.results[].name'
# tags (GHCR)
gh api "/orgs/<org>/packages/container/<name>/versions" --jq '.[].metadata.container.tags[]'
# arches, without pulling
docker manifest inspect <image>:<tag> | jq -r '.manifests[].platform | "\(.os)/\(.architecture)"'
# config, after a pull
docker image inspect <image>:<tag> --format '{{json .Config}}' | jq
```

If no prebuilt image fits, establish what building one costs: build system, build-time
dependencies, whether upstream ships a usable `Dockerfile`, and whether it
cross-compiles for arm64.

## 3 · Runtime dependencies

For each of database, cache, search index, object storage, message broker, and any
external API:

- Is it **required or optional**, and what degrades without it.
- What upstream **recommends** for self-hosters, versus what merely works.
- Can it be **embedded** (SQLite, an in-process cache) instead of run as a sidecar?
- If it must be a sidecar, which recipe covers it — `recipe-postgresql.md`,
  `recipe-mysql.md`, `recipe-valkey.md`, `recipe-multi-daemon.md`.
- Pinned versions and version floors, and whether upstream supports the version you
  would ship.
- A dependency on **another StartOS service** (Bitcoin, LND, CLN…) is a different thing
  entirely — `recipe-dependency.md`, and in user-facing text a multi-flavour dependency
  is named generically.
- **Anything that must reach the public internet** to function at all — a hosted API, a
  license check, a mandatory relay. Name it; it changes what the package can promise.

## 4 · Configuration surface

- **Mechanism**: environment variables, a config file, a database table, a web settings
  page, or a mix. For a file: exact format, exact path inside the container, and whether
  the app rewrites the whole file on shutdown.
- The settings that **must be fixed** by the package rather than exposed: bind address,
  listen port, data directory, base URL, proxy trust, TLS termination.
- The settings a user genuinely needs to reach — those become actions
  (`recipe-config-actions.md`), and their file representation becomes a file model
  (`file-models.md`).
- Settings that are traps: telemetry/analytics on by default, auto-update, phone-home
  version checks, crash reporting.
- Anything the app **generates on first run** and then depends on — a secret key, an
  instance id, a device id.

Read upstream's own `docker-compose.yml`, `.env.example`, and config reference. Those
three files usually settle this section on their own.

## 5 · Secrets and credentials

- Which secrets exist (app secret key, DB password, JWT signing key, encryption key) and
  which the **package must generate** — `recipe-internal-secrets.md`.
- Whether a secret is **stable for the life of the install**. Regenerating an encryption
  key on restart destroys data; that has to be understood before it is written.
- Whether an admin credential can be applied **non-interactively** — an env var, the
  app's CLI, its API — or only through a web setup wizard.
- **Never invent an on-disk credential format.** Confirm what the app actually writes, or
  apply the credential through the app's own mechanism.

## 6 · Network surface

- **Every** port, with protocol and purpose: web UI, API, metrics, peer/P2P, websocket,
  SMTP/IMAP, database wire, sync. A port you don't bind is a feature that silently
  doesn't work.
- Which are `ui`, which are `api`, which are `p2p` — a label for the user, not a control.
- Whether the service needs a **stable public URL** it knows about (federation, invites,
  OAuth callbacks, webhooks). If so, `recipe-primary-url.md`.
- Outbound requirements: does it need to reach the internet, a LAN device, or another
  service on the box.

## 7 · Auth and the first-run flow

- What the user meets on first load: a setup wizard, a login with default credentials, an
  open instance awaiting registration, nothing at all.
- Can an admin be created before first load? If not, the package's job is to _point_ the
  user at the flow, not to fake it.
- **Registration gating** — can public signup be disabled, and how
  (`recipe-registration-gating.md`).
- OIDC / LDAP / SSO support, and whether any of it is needed for a good default.
- Whether a password can be **reset** from outside the UI — that determines whether a
  reset action is possible (`recipe-reset-password.md`).

## 8 · Data and persistence

- **Every** path the container writes that must survive a restart, and what lives in
  each. Config and data are usually separate paths; missing one loses data silently
  rather than erroring.
- Which paths are cache or derived and should _not_ be backed up.
- Whether the data store needs a **consistent dump** rather than a file copy (`pg_dump`,
  `mysqldump`, an app-level export) — `recipe-backups.md`.
- File ownership and mode the app expects, and whether it fixes them itself (`PUID`/
  `PGID` on `linuxserver/*` images) or needs a `chown` oneshot.
- Rough data growth: does this become hundreds of gigabytes.

## 9 · Behind a reverse proxy

StartOS fronts every service with its own proxy, so the request the app sees has a
different `Host`, `Origin` and port than it served.

- Host-header or `Origin`/CSRF validation that rejects proxied requests — and the setting
  that relaxes it.
- Whether it needs `X-Forwarded-For` / `X-Forwarded-Proto` honoured, and the setting that
  enables trusting them.
- Whether it emits **absolute URLs** built from a configured base URL, and where that is
  set.
- Any "trust localhost" auth bypass — proxy-local requests would skip the password
  entirely. Disable it.
- Websocket upgrade requirements.

## 10 · Upgrades

- Does the app **migrate its own schema** on start, and does it do so safely
  unattended?
- Are there **version-skew rules** — "you must upgrade through 2.x before 3.x", "never
  skip a major"? That shapes how the package can bump upstream at all.
- Is downgrading safe, or does an upgrade write data an older version can't read?
- Known breaking releases, and any manual step upstream documents around them.

## 11 · Footprint

RAM and CPU at idle and under normal use, disk for the image and for data, and whether it
wants a GPU. StartOS servers are modest — a service that needs 8 GB at idle is a fact for
the gate, not a detail.

## 12 · What self-hosters actually do

The layer that separates a working package from a good one. Read upstream's self-hosting
docs, the project's own compose file, its discussions/issues, and what the self-hosted
community writes about it:

- The **deployment shape people actually run** — which services in the compose stack,
  which are optional in practice.
- The settings everybody changes immediately, and the defaults everybody regrets.
- The **recurring support questions** — those are the ones `instructions.md` should
  pre-empt.
- Known-bad configurations and footguns.
- Whether there is a "hardened" or "recommended for self-host" posture upstream
  documents.

## 13 · Fit with the fleet

- Does an existing Start9 or community package overlap, complement, or become a
  dependency of this one? Check the Start9 and community registry indexes, and the
  package repos already in the workspace.
- Does an existing package already solve part of this (a database service, a proxy, a
  relay)?
- Which registry **category** it belongs to.

## 14 · What you could not establish

Carry this list to the gate explicitly. "I could not confirm whether the admin password
can be set non-interactively" is a question the user can answer or authorize you to resolve
by experiment. Silence there is how a package gets built on a guess.
