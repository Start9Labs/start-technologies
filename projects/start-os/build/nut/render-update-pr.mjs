import { readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const directory = process.argv[2]
if (!directory) throw Error('Expected the verified update artifact directory')
const report = JSON.parse(readFileSync(join(directory, 'report.json'), 'utf8'))
const run = `${process.env.GITHUB_SERVER_URL}/${process.env.GITHUB_REPOSITORY}/actions/runs/${process.env.GITHUB_RUN_ID}`
let text = `## Summary

Update the pinned NUT source from \`${report.current}\` to \`${report.candidate}\`.

\`\`\`text
verified source → pinned Trixie rebuild → normal StartOS update
\`\`\`

[Upstream release notes](${report.releaseUrl}).

## Evidence

- **Before:** \`${report.current}\` was the selected Debian source.
  **After:** \`${report.candidate}\` is verified against the pinned maintainer's signature,
  signed archive checksums and the upstream release digest.
- [Completed validation](${run}): source-updater regressions, all three architecture
  builds, fresh/upgrade installation, cross-version reads and shutdown-marker tests.

## Merge Danger

**Door:** two-way

The prior complete StartOS rootfs provides rollback.

**Blast Radius:** UPS

Changes host UPS drivers, server, clients and libraries together. Devices receive
the change through a StartOS release; this workflow does not deploy it.
`
if (readFileSync(join(directory, 'release-aligned'), 'utf8').trim() !== 'true')
  text += `
This PR is a draft because the new fragment requires StartOS release-version
alignment. Follow \`shared-libs/crates/start-core/VERSION_BUMP.md\` before marking
it ready; this automation does not invent a migration node or release version.

\`\`\`text
${readFileSync(join(directory, 'release-check.txt'), 'utf8')}
\`\`\`
`
writeFileSync(join(directory, 'pr-body.md'), text)
