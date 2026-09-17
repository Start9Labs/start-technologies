# The decision gate

The one round of questions that turns research into a buildable spec. Ask everything you
will need; you do not get a second round.

## Shape of the ask

- **A recommendation on every question.** The user should be able to answer the whole
  gate with "go with your recommendations" and get a package they want.
- **Discrete choices as numbered options, your recommendation first and labelled
  `(Recommended)`.** Use a structured question tool where the harness has one (Claude
  Code's `AskUserQuestion`: at most four questions per call, header ≤ 12 characters,
  `preview` only where seeing the two shapes side by side actually decides it — a daemon
  topology, a config file, an interface set); otherwise number them in prose.
- **Prose beneath for the open-ended ones.** Those are where the shape actually comes
  from; the multiple-choice questions mostly confirm it.
- **Decide, don't ask, when the answers produce the same code.** Say what you decided
  instead. A gate padded with settled questions buries the two that matter.
- **Ask about consequence, not implementation.** "Ship a Postgres sidecar, or use SQLite
  and cap it at a few thousand items?" is the user's call. "Which zod schema shape" is
  not.

## The catalog

Ask what the research made live. Skip what it settled.

### Scope

- **Where it lands** — the GitHub org that will publish it (`Start9Labs`,
  `Start9-Community`, or the user's own), and the directory under the workspace if
  `AGENTS.local.md` sorts packages into more than one. Default: a `<id>-startos/` repo at
  the workspace root.
- **Display name and package id.** `init-package` derives the id from the display name;
  confirm both when the obvious id is taken, ambiguous, or ugly.
- **Registry category.** Take the list from a registry index; recommend one.

### Image and build

- **Which image**, when more than one is credible — upstream official vs `linuxserver/*`
  vs a community build. Give the trade-off in one line each.
- **Pin a version tag or track a moving one.** Recommend a pinned semver tag; say what
  `UPDATING.md` will tell a future maintainer to watch.
- **Prebuilt image vs a `Dockerfile`**, when no prebuilt image covers both arches or
  upstream ships none.

### Topology

- **Sidecars.** Postgres vs SQLite, a cache daemon vs none, a worker process vs
  in-process. Frame it as what the user gains and what it costs in RAM.
- **StartOS service dependencies** — required, optional, or alternatives the user picks
  between. Name the multi-flavour ones generically (Bitcoin, not Bitcoin Core).
- **Optional upstream components** the compose stack includes that the package could
  drop — an ML worker, a metrics exporter, a second frontend.

### Feature set

- **What ships in v1 and what waits.** Enumerate the optional capabilities you found and
  recommend a line through them. This is usually the most valuable question at the gate.
- **Anything that needs a public domain to work** (federation, OAuth callbacks, invite
  links). A box with a real domain can exercise it; without one it stays reasoned about —
  say which, and whether that changes what you recommend shipping. It changes the
  first-run flow either way.
- **Telemetry, auto-update, and phone-home** — recommend off, and say what turning them
  off costs.

### Configuration exposed to the user

- **Which settings become actions**, and which the package fixes. Recommend the smallest
  set that covers the recurring support questions from the research, and say what you are
  deliberately locking down.
- **SMTP** — off, StartOS system SMTP, or user-supplied (`recipe-smtp.md`).
- **Registration gating**, when the service has public signup.

### Credentials and first run

- **Who creates the admin account** — the package via a critical task and a
  `setAdminPassword` action (`recipe-admin-credentials.md`), or the app's own setup
  wizard with the package pointing at it. Driven by whether a credential can be applied
  non-interactively; if it can, recommend the package doing it.
- **A password reset action**, when the app supports resetting from outside the UI.

### Interfaces

- **Which ports become interfaces**, and each one's `type`. List every port you found and
  recommend the set — including the ones you propose _not_ to expose, and why.

### Backup

- **What is backed up** — the whole volume, selected subpaths, or a dump plus files —
  and what is deliberately excluded as cache or derived. Recommend from the research;
  ask when the data store needs a consistent dump.

### Verification

Only what Phase 0 and `AGENTS.local.md` left open — settle it here, because nothing gets
asked once the build starts:

- **Which box**, when the workspace names none or more than one.
- **The backup target and password** for the round-trip, when the workspace's rules
  don't supply them. Scope the backup to this package either way — the target may hold
  the user's real backups.
- **Whether a public domain is available** on the box, for anything gated on one.

## Open-ended prompts worth asking

Pick the ones the research left live:

- Anything in the research that surprises you or that you would rather I handled
  differently?
- Is there a user you have in mind for this — which of its uses should the defaults
  favour?
- How opinionated should the package be: lock it down to one good configuration, or
  expose the knobs and let people break it?
- Anything about this service you already know that the research would not turn up?

## Before you close the gate

State plainly, in one block:

- the spec you will build to, in enough detail that the user can catch a wrong reading
- what you are deliberately **not** shipping
- the facts you could not establish, and how you intend to resolve each — by experiment
  during the build, or by shipping a conservative default
- what you will verify on the box, and what you will not be able to
- that you will not check in again until the package installs and runs on the box
