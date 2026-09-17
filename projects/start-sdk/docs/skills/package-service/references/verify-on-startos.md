# Verify on a StartOS box

The protocol: what to **prove**, in order, and what each step catches. The box is the one
Phase 0 confirmed — `host.default` in `.startos/config.yaml`, or the host
`AGENTS.local.md` names — and every `start-cli` call below takes it (`-H <profile|url>`,
omitted when it is the default). `AGENTS.local.md` outranks the commands here wherever
it gives its own.

---

## 0 · Build

```bash
make x86            # or `make arm`, for the box's architecture
```

Fix every `tsc` and pack error. A pre-existing error is still an error.

## 1 · Install

```bash
make x86 install    # builds and installs to host.default; see makefile.md
```

Or pack and install explicitly against a named host:

```bash
start-cli -H <host> s9pk pack --arch=x86_64 -o <id>_x86_64.s9pk
start-cli -H <host> package install -s <id>_x86_64.s9pk
```

Then drive it with `start-cli -H <host>` — start, stop, actions, logs, health. Watch the
install itself: a package that installs with errors in the log has not installed cleanly.

**Catches:** manifest and asset problems, an image that won't pull for the arch, a
signing or pack fault.

## 2 · First run

Follow the flow a real user meets, in the order they meet it: the tasks the package
surfaces, the action a critical task points at, the credential it hands back, the first
load of the UI.

**Catches:** a critical task that blocks startup and hides everything else; a credential
flow that was never actually logged into.

## 3 · Health and interfaces

Start the service. Every daemon comes up, every health check goes green, and it _stays_
green — watch it past the first minute.

Open **every** interface you declared, not just the UI. An interface that resolves but
serves nothing is a bound port, not a working interface.

**Catches:** a `ready` check that passes on a port that is listening but not serving; an
interface bound to the wrong container port; a service that comes up and then crash-loops.

## 4 · Actions and tasks

Run **every** action end to end and read what it returns. Two `start-cli` gotchas:

- `start-cli package action run <id> <action-id>` always reads its input from stdin, so
  a `withoutInput` action still needs `<<< 'null'` — bare, it dies with
  `Deserialization Error: EOF while parsing a value`.
- An action _with_ input must first be primed by
  `start-cli package action get-input <id> <action-id>`, whose `eventId` you hand back
  as `--event-id`; skip it and the run fails with
  `getActionInput has not been called for EventID …`.

Confirm each task appears when it should and clears when its action runs.

**Catches:** an action that compiles and does nothing; a task whose completion condition
never becomes true, leaving a permanent nag or a permanently blocked service.

## 5 · Real use

Use the service for what it is for. Log in. Create something. Upload a file. Add a feed,
a user, a wallet, a note — whatever this service exists to hold.

**Catches:** everything a smoke test doesn't. A service that loads its own login page is
not a working package.

## 6 · Restart persistence

Stop the service, start it, and confirm what you created in step 5 is still there.

**Catches:** the single most common prebuilt-image defect — a data path that was never
mounted. It produces no error and passes every test that doesn't restart.

## 7 · Backup and restore

Round-trip it: back up, then restore, then start the package and confirm the data from
step 5 came back.

```bash
start-cli -H <host> backup create <target-id> '<password>' --package-ids <id>
start-cli -H <host> package backup restore <target-id> '<password>' <id>
start-cli -H <host> package start <id>
```

Two rules, both load-bearing: **scope the backup to this package** — `--package-ids` is
additive, so a scoped run leaves every other package's backup on that target intact,
and the target may hold the user's real backups — and **start the package after a
restore**, which reinstalls it stopped.

**Catches:** a backup that captures the config and not the database; a restore that comes
back empty; an app whose data store needed a consistent dump and got a file copy.

## 8 · Teardown and reinstall

Uninstall, confirm the volume directory is gone, and install once more from the same
`.s9pk`. The second install must be as clean as the first.

**Catches:** state left outside the package's volumes; an init path that only works
against a dirty box.

## 9 · Logs

Read the service log end to end at least once. Warnings the app emits about its own
configuration — a deprecated setting, an untrusted proxy, a missing secret, a failed
migration — are defects to run down, not noise.

---

## Working through failures

- **Look inside the container** rather than guessing: `start-cli package attach <id> -n
<subcontainer-name> -- <cmd>`. `-n` takes the subcontainer's name; `-s` takes its
  internal Guid and fails on a name.
- **A package's volumes on the box are at
  `/media/startos/data/package-data/volumes/<id>/data/<volume>/`** — where to check the
  ownership and modes the package's own code sets. `package uninstall` deletes the
  directory.
- **A `401` behind the proxy is usually not a bad password.** Host-header and CSRF guards
  reject proxied requests and say very little about it — read the app's own log for the
  real reason. Section 9 of the research checklist is where the fix comes from.
- **Config you write while the app runs can be clobbered** by the app's shutdown flush.
  Write config-file changes from `setupMain` before the daemon launches, or apply them
  through the running app's API.
- **A feature gated on a public domain** can only be exercised on a box that has one.
  Register it on the package's host (`start-cli package host <id> address <host-id>
domain public add …`) when the workspace provides a domain; otherwise say at
  hand-back that it was reasoned about, not driven.
- **Iterate with a dirty tree**; install again. Don't commit between attempts.
- **Anything you register on the box that isn't the package** — a public domain, a
  scratch container — is yours to report at hand-back.

## Reporting

Say what you verified and how you verified it, step by step. Say what you did not verify
and why. If a step could not be run at all — the box unreachable, a feature that needs
something the box doesn't have — report exactly how far you got.

Never report a package green on the strength of a green `tsc`.
