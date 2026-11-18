import { chmodSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { REPO, fixture } from './fixtures.mjs'

export function artifactFixture(t, runs) {
  const f = fixture(t)
  for (const helper of [
    'scripts/gha-artifact-run-ids.sh',
    'scripts/gha-artifact-download.sh',
  ])
    chmodSync(f.write(helper, readFileSync(join(REPO, helper))), 0o755)
  f.write('runs.json', JSON.stringify(runs))
  f.write('gh.log', '')
  f.write('downloads.log', '')
  f.nodeExecutable(
    'gh',
    `import { appendFileSync, readFileSync } from 'node:fs'
import { spawnSync } from 'node:child_process'
const root = ${JSON.stringify(f.root)}
const runs = JSON.parse(readFileSync(root + '/runs.json'))
const args = process.argv.slice(2)
appendFileSync(root + '/gh.log', JSON.stringify(args) + '\\n')
const artifacts = run => run.artifacts.map((artifact, index) => ({
  id: run.id * 1000 + index,
  expired: false,
  created_at: run.created_at ?? '',
  ...(typeof artifact === 'string' ? { name: artifact } : artifact),
  workflow_run: { id: run.id, head_branch: run.head_branch },
}))
const newestFirst = (a, b) =>
  b.created_at.localeCompare(a.created_at) || b.workflow_run.id - a.workflow_run.id
if (args[0] === 'api') {
  const url = args.find(arg => arg.startsWith('repos/'))
  if (process.env.GH_FAIL_MATCH && url.includes(process.env.GH_FAIL_MATCH)) process.exit(17)
  const page = Number(new URL('https://x/' + url).searchParams.get('page') ?? 1)
  let response
  const workflow = url.match(/workflows\\/([^/]+)\\/runs/)
  const run = url.match(/actions\\/runs\\/(\\d+)(\\/artifacts)?/)
  const zip = url.match(/actions\\/artifacts\\/(\\d+)\\/zip$/)
  if (/\\/actions\\/artifacts(\\?|$)/.test(url)) {
    const all = runs.flatMap(artifacts).sort(newestFirst)
    response = { artifacts: all.slice((page - 1) * 100, page * 100) }
  } else if (zip) {
    const found = runs.flatMap(artifacts).find(artifact => String(artifact.id) === zip[1])
    if (!found) process.exit(22)
    appendFileSync(root + '/downloads.log', found.workflow_run.id + ' ' + found.name + '\\n')
    process.stdout.write(found.name + ' contents\\n')
    process.exit(0)
  } else if (workflow) {
    const matching = runs.filter(run => run.path === '.github/workflows/' + workflow[1])
      .sort((a, b) => b.created_at.localeCompare(a.created_at))
    response = { workflow_runs: matching.slice((page - 1) * 100, page * 100) }
  } else if (run) {
    const found = runs.find(candidate => String(candidate.id) === run[1])
    if (!found) process.exit(22)
    const { artifacts: ignored, ...metadata } = found
    response = run[2] ? { artifacts: artifacts(found) } : metadata
  } else process.exit(2)
  if (args.includes('--jq')) {
    const result = spawnSync('jq', ['-r', args[args.indexOf('--jq') + 1]], {
      input: JSON.stringify(response), encoding: 'utf8',
    })
    process.stdout.write(result.stdout)
    process.stderr.write(result.stderr)
    process.exit(result.status)
  }
  console.log(JSON.stringify(args.includes('--slurp') ? [response] : response))
} else process.exit(2)
`,
  )
  const lines = path =>
    readFileSync(join(f.root, path), 'utf8').trim().split('\n').filter(Boolean)
  return {
    ...f,
    env: { ...f.env, PATH: `${f.root}:${f.env.PATH}` },
    calls: () => lines('gh.log').map(line => JSON.parse(line)),
    downloads: () => lines('downloads.log'),
  }
}
