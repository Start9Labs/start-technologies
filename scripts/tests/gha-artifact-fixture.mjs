import { chmodSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { REPO, fixture } from './fixtures.mjs'

export function artifactFixture(t, runs) {
  const f = fixture(t)
  const helper = 'scripts/gha-artifact-run-ids.sh'
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
const artifacts = run => run.artifacts.map(artifact => ({
  expired: false,
  ...(typeof artifact === 'string' ? { name: artifact } : artifact),
  workflow_run: { id: run.id, head_branch: run.head_branch },
}))
if (args[0] === 'api') {
  const url = args.find(arg => arg.startsWith('repos/'))
  if (process.env.GH_FAIL_MATCH && url.includes(process.env.GH_FAIL_MATCH)) process.exit(17)
  let response
  const workflow = url.match(/workflows\\/([^/]+)\\/runs/)
  const run = url.match(/actions\\/runs\\/(\\d+)(\\/artifacts)?/)
  if (url.endsWith('/actions/artifacts')) {
    const name = args.find(arg => arg.startsWith('name=')).slice(5)
    const matching = runs.flatMap(artifacts).filter(artifact => artifact.name === name)
    response = { artifacts: args.includes('--paginate') ? matching : matching.slice(0, 100) }
  } else if (workflow) {
    const page = Number(new URL('https://x/' + url).searchParams.get('page') ?? 1)
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
} else if (args[0] === 'run' && args[1] === 'download') {
  appendFileSync(root + '/downloads.log', args[4] + ' ' + args[args.indexOf('-p') + 1] + '\\n')
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
