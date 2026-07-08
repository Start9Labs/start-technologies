const path = require('path')
const core = require('@actions/core')
const glob = require('@actions/glob')
const { DefaultArtifactClient } = require('@actions/artifact')

async function run() {
  const pattern = core.getInput('pattern', { required: true })
  const retentionRaw = core.getInput('retention-days')
  const retentionDays = retentionRaw ? parseInt(retentionRaw, 10) : 0
  const compressionLevel = parseInt(
    core.getInput('compression-level') || '6',
    10,
  )
  const ifNoFilesFound = (
    core.getInput('if-no-files-found') || 'warn'
  ).toLowerCase()

  const globber = await glob.create(pattern, { matchDirectories: false })
  const files = (await globber.glob()).filter(f => !f.endsWith(path.sep))

  if (files.length === 0) {
    const msg = `No files matched pattern: ${pattern}`
    if (ifNoFilesFound === 'error') return core.setFailed(msg)
    if (ifNoFilesFound === 'warn') return core.warning(msg)
    return core.info(msg)
  }

  const client = new DefaultArtifactClient()
  const cwd = process.cwd()
  const uploaded = []

  for (const file of files) {
    const name = path.basename(file)
    const rel = path.relative(cwd, file) || file
    core.info(`Uploading ${name} (${rel})`)
    const opts = { compressionLevel }
    if (retentionDays > 0) opts.retentionDays = retentionDays
    const result = await client.uploadArtifact(name, [file], cwd, opts)
    core.info(`  artifact id=${result.id} size=${result.size}`)
    uploaded.push(name)
  }

  core.setOutput('artifact-names', JSON.stringify(uploaded))
  core.info(`Uploaded ${uploaded.length} artifact(s).`)
}

run().catch(err => core.setFailed(err && err.stack ? err.stack : String(err)))
