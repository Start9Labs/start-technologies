# New Package Checklist

`start-cli s9pk init-package "<Name>"` scaffolds a barebones clone — one daemon running the
hello-world image with a port-listening health check, no interface, no dependencies. This page
takes that clone to a release-ready package. Work it top to bottom.

The scaffold names its arbitrary ids `example-*` (`example-volume`, `example-image`,
`example-daemon`) to signal that you rename them freely; they are not required namings.

> [!TIP]
> Wrapping an existing upstream Docker image — the common case? Read
> [Package a Prebuilt Docker Image](recipe-prebuilt-image.md) first. It expands the
> "replace the hello-world image" step below.

## Identity & metadata

- `startos/manifest/index.ts`: fill in `packageRepo`, `upstreamRepo`, and `marketingUrl` /
  `donationUrl` (or remove the latter two). Confirm the `license`.
- Replace the placeholder `LICENSE` file with your package's license, matching the `license`
  field in `startos/manifest/index.ts`.
- `startos/manifest/i18n.ts`: write the short and long descriptions, then translate them into
  the other locales.
- Replace `icon.svg` with a real icon for your service (≤ 40 KiB). Fetch the upstream asset —
  never ship an invented one.

## The service

- Rename the `example-*` placeholder ids to fit your service. Keep them consistent across
  `startos/manifest/index.ts` (the `example-image` key and `example-volume` entry),
  `startos/main.ts` (`imageId`, `volumeId`, the daemon and subcontainer ids), and
  `startos/backups.ts` (the backed-up volume).
- Replace the hello-world image with your service's image: set `images.*.source.dockerTag`
  (or add a `Dockerfile`) in `startos/manifest/index.ts`, and update the `exec.command` in
  `startos/main.ts`. Document how you track that version in `UPDATING.md`.
- `startos/main.ts`: define the daemon(s) and any oneshots. The example daemon ships a
  `checkPortListening` health check on `uiPort` (`startos/utils.ts`) — point `uiPort` at the
  port your service listens on, or swap in another check. Keep only the i18n keys in
  `startos/i18n/dictionaries` that you actually reference.
- Interfaces: `startos/interfaces.ts` ships wired into `startos/init/index.ts` but returns an
  empty list. If the service exposes a network interface, bind a port and export the interface
  there (see [Interfaces](interfaces.md)).
- `startos/backups.ts`: choose what to back up (see [Back Up and Restore Data](recipe-backups.md)).
- `startos/dependencies.ts`: declare any dependencies (or confirm none).
- `startos/actions/`: add user-facing actions / config as needed (see
  [Create Configuration Actions](recipe-config-actions.md)).
- `startos/init/`: add install / restore setup if the service needs it.
- `startos/versions/`: set the initial version string and release notes.

## Docs

- Write `README.md` (per [Writing READMEs](writing-readmes.md)).
- Write `instructions.md` (per [Writing Instructions](writing-instructions.md)).
- Fill in `UPDATING.md` — what "upstream" means for this package, where the pin lives, and how
  to bump it.
- Write the `## This repo` bullets in `AGENTS.md`, or delete the section. A simple package
  needs none.

## Build, test, ship

- First test build: `make` (or `start-cli s9pk pack`); fix any `tsc` / pack errors.
- Install on a StartOS box and verify the service runs, and is reachable once it exposes an
  interface. A green `tsc` proves the code builds, not that the service works — see
  [Development Workflow](workflow.md#verify-against-reality-not-against-tsc).
- Backup / restore sanity check.
- Review the README and instructions one more time against actual behavior.
- Publish (see [Publishing](publishing.md)).

## After that

The checklist ends here, and it leaves nothing behind in the repo to maintain. From this point
a defect or a wanted capability is **a GitHub issue on the package repo** — not a checklist, a
worklist, or a notes file. See
[Development Workflow — Bugs and feature requests are issues](workflow.md#bugs-and-feature-requests-are-issues).
