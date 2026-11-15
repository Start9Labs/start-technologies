# NUT update proposals

The [NUT Updates workflow](../../../../.github/workflows/nut-updates.yml) checks
every Tuesday at 05:23 UTC, or on manual dispatch from `master`. It discovers
stable upstream releases and newer Debian source revisions and proposes verified
source-pin updates for maintainer review.

The workflow becomes active when the package integration supplies
`projects/start-os/build/nut/version.env`, `make start-os-nut` and
`make start-os-nut-test`. Without the pin file, discovery exits successfully with
a waiting report and performs no downloads or publication. The updater and its
offline fixture tests can land independently of the package integration.

The updater verifies the pinned Debian maintainer signature, downloaded source
sizes and SHA-256 hashes, runtime package names and GitHub's upstream archive
digest. A release without matching Debian source is reported as waiting. A changed
signer, archive format or missing upstream digest requires manual review. Verified
updates retain the builder image, signer, package set and Trixie target.

For each source revision, all amd64, arm64 and riscv64 package builds and fresh/
upgrade installation checks must pass before publication. The updater revises
the pending UPS feature fragment when present, otherwise adds a routine patch
fragment. A failed StartOS release-version check leaves the proposal in draft
with the check output and the version-bump procedure. Maintainers own release
versions, migration nodes, merge and deployment.

Each source revision has one proposal, including closed PRs. Interrupted
publication can resume; maintainer changes are preserved for manual review.
Publication uses the shared signed-commit API helper and `GITHUB_TOKEN`, with
write permissions confined to the proposal job. Enable **Allow GitHub Actions
to create and approve pull requests** in repository Actions settings. No new
secret or GitHub App is needed. Generated PR workflows may require approval;
the three package architecture checks already ran before proposal creation.

Run `make start-os-nut-update-check` for a read-only check, or
`node projects/start-os/build/nut/update.mjs --write` to apply verified pins and
the fragment locally. `make start-os-nut-update-test` runs signed-source and
publication regressions with Node.js, GnuPG, dpkg, Git, jq and root npm
dependencies. It is included in `make start-os-scripts-test` and uses synthetic
pins and signing keys independent of the selected production NUT version.
