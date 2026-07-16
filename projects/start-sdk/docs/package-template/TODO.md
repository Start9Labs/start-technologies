# TODO — bring {{name}} from template to release-ready

This package was scaffolded as a barebones clone — one daemon running the hello-world
image with a port-listening health check, no interface yet, no dependencies. The arbitrary ids are named
`example-*` (e.g. `example-volume`, `example-image`, `example-daemon`) to signal that you
rename them freely; they are not required namings. Work the list top to bottom; it takes
you from the clone to a release-ready package. Consult the packaging guide as you go
(`start-technologies/projects/start-sdk/docs/src/recipes.md` is the intent index). Remove items as you finish
them, and add items when you defer work.

## Identity & metadata

- [ ] `startos/manifest/index.ts`: fill in `packageRepo`, `upstreamRepo`, and
      `marketingUrl` / `donationUrl` (or remove the latter two). Confirm the `license`.
- [ ] Replace the placeholder `LICENSE` file with your package's license, matching the
      `license` field in `startos/manifest/index.ts`.
- [ ] `startos/manifest/i18n.ts`: write the short and long descriptions, then translate
      them into the other locales.
- [ ] Replace `icon.svg` with a real {{name}} icon (≤ 40 KiB).

## The service

- [ ] Rename the `example-*` placeholder ids to fit your service. Keep them consistent
      across `startos/manifest/index.ts` (the `example-image` key and `example-volume` entry),
      `startos/main.ts` (`imageId`, `volumeId`, the daemon and subcontainer ids), and
      `startos/backups.ts` (the backed-up volume).
- [ ] Replace the hello-world image with your service's image: set `images.*.source.dockerTag`
      (or add a `Dockerfile`) in `startos/manifest/index.ts`, and update the `exec.command` in
      `startos/main.ts`. (`UPDATING.md` should document how you track the version.)
- [ ] `startos/main.ts`: define the daemon(s) and any oneshots. The example daemon ships a
      `checkPortListening` health check on `uiPort` (`startos/utils.ts`) — point `uiPort` at the
      port your service listens on, or swap in another check. Keep only the i18n keys in
      `startos/i18n/dictionaries` that you actually reference.
- [ ] Interfaces: `startos/interfaces.ts` ships wired into `startos/init/index.ts` but
      returns an empty list. If the service exposes a network interface, bind a port and
      export the interface there (see `start-technologies/projects/start-sdk/docs/src/interfaces.md`).
- [ ] `startos/backups.ts`: choose what to back up.
- [ ] `startos/dependencies.ts`: declare any dependencies (or confirm none).
- [ ] `startos/actions/`: add user-facing actions / config as needed.
- [ ] `startos/init/`: add install / restore setup if the service needs it.
- [ ] `startos/versions/`: set the initial version string and release notes.

## Docs

- [ ] Write `README.md` (per `start-technologies/projects/start-sdk/docs/src/writing-readmes.md`).
- [ ] Write `instructions.md` (per `start-technologies/projects/start-sdk/docs/src/writing-instructions.md`).
- [ ] Fill in `UPDATING.md` (upstream-version tracking).

## Build, test, ship

- [ ] First test build: `make` (or `start-cli s9pk pack`); fix any `tsc` / pack errors.
- [ ] Install on a StartOS box and verify the service runs (and is reachable, once it
      exposes an interface).
- [ ] Backup / restore sanity check.
- [ ] Review the README and instructions one more time against actual behavior.
