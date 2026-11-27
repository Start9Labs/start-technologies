import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import {
  chmodSync,
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import test from 'node:test'

const tools = fileURLToPath(new URL('../', import.meta.url))
const env = { PATH: process.env.PATH }

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'complexity-test-'))
  t.after(() => rmSync(root, { recursive: true, force: true }))
  const put = (path, text) => {
    mkdirSync(dirname(join(root, path)), { recursive: true })
    writeFileSync(join(root, path), text)
  }
  put('shared-libs/.keep', '')
  put('projects/service/lib/utils.ts', 'export function utility() {}\n')
  put('projects/client/src/net/a.ts', 'function run() { utility() }\n')
  put('projects/client/src/ui/b.ts', 'utility()\néutility: 1,\n')
  const fn = (name, cognitive, sloc, cyclomatic = 1, spaces = []) => ({
    kind: 'function',
    name,
    start_line: 1,
    spaces,
    metrics: {
      cognitive: { value: cognitive },
      cyclomatic: { value: cyclomatic },
      loc: { sloc },
    },
  })
  const doc = (path, spaces) => ({
    kind: 'unit',
    name: join(root, path),
    spaces,
  })
  const config = {
    docs: [
      doc('projects/service/lib/utils.ts', [
        fn('utility', 26, 10, 2, [fn('', 3, 2)]),
      ]),
      doc('projects/client/src/net/a.ts', [fn('run', 25, 5, 3)]),
      ...[
        'thing.spec.ts',
        'thing.d.ts',
        'osBindings/types.ts',
        'dist/index.ts',
      ].map(path => doc('projects/service/' + path, [fn('excluded', 99, 99)])),
    ],
    total: 1000,
    errors: 0,
  }
  put('config.json', JSON.stringify(config))
  put(
    'fake-bca.mjs',
    `#!/usr/bin/env node
import { readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
const root = fileURLToPath(new URL('.', import.meta.url))
const config = JSON.parse(readFileSync(join(root, 'config.json'), 'utf8'))
const args = process.argv.slice(2)
writeFileSync(join(root, args[0] + '-args.json'), JSON.stringify(args))
if (config[args[0] + 'Failure']) {
  console.error(args[0] + ' exploded')
  process.exit(9)
}
if (args[0] === 'metrics') {
  const out = args[args.indexOf('--output-dir') + 1]
  config.docs.forEach((doc, i) => writeFileSync(join(out, i + '.json'), JSON.stringify(doc)))
} else {
  console.log('Total nodes: ' + config.total)
  console.log('Found nodes: ' + config.errors)
}
`,
  )
  chmodSync(join(root, 'fake-bca.mjs'), 0o755)
  const update = changes =>
    put('config.json', JSON.stringify({ ...config, ...changes }))
  const run = (...args) =>
    spawnSync(
      process.execPath,
      [
        join(tools, 'census.mjs'),
        '--root',
        root,
        '--bca',
        join(root, 'fake-bca.mjs'),
        ...args,
      ],
      { encoding: 'utf8', env },
    )
  return { root, put, config, update, run }
}

function succeeded(result) {
  assert.equal(result.status, 0, result.stderr)
  return result.stdout
}

test('census preserves nested metrics, exclusions, callers and subsystem adoption', t => {
  const f = fixture(t)
  const report = JSON.parse(succeeded(f.run('--json')))
  assert.deepEqual(report.totals, {
    functions: 3,
    cognitive: 54,
    cyclomatic: 6,
    sloc: 17,
    over25: 1,
  })
  assert.deepEqual(
    report.functions.find(row => row.name === 'utility'),
    {
      file: 'projects/service/lib/utils.ts',
      name: 'utility',
      line: 1,
      cognitive: 26,
      cyclomatic: 2,
      sloc: 10,
      callers: 2,
      shared: true,
      util: true,
      scopes: ['projects/client/net', 'projects/client/ui'],
    },
  )
  assert.equal(
    report.functions.find(row => row.name === '<anonymous>').callers,
    undefined,
  )
  assert.equal(report.functions.find(row => row.name === 'run').callers, 0)
  assert.equal(succeeded(f.run('--totals')), '3\t54\t6\t17\t1\n')
  const top = succeeded(f.run('--top', '1')).trimEnd().split('\n')
  assert.equal(top.length, 2)
  assert.match(
    top[1],
    /26\s+10\s+utility\s+projects\/service\/lib\/utils.ts:1$/,
  )
  const args = JSON.parse(
    readFileSync(join(f.root, 'metrics-args.json'), 'utf8'),
  )
  assert.ok(args.includes('--exclude-tests'))
  assert.deepEqual(args.slice(-2), [
    join(f.root, 'shared-libs'),
    join(f.root, 'projects'),
  ])
  succeeded(
    f.run(
      '--totals',
      '--scope',
      'projects/client',
      '--scope',
      'projects/service',
    ),
  )
  assert.deepEqual(
    JSON.parse(readFileSync(join(f.root, 'count-args.json'), 'utf8')).slice(-2),
    [join(f.root, 'projects/client'), join(f.root, 'projects/service')],
  )
})

test('analyzer failures and missing functions cannot look like a clean census', t => {
  const f = fixture(t)
  for (const [changes, message] of [
    [{ metricsFailure: true }, /metrics.*failed.*metrics exploded/],
    [{ countFailure: true }, /count.*failed.*count exploded/],
    [{ docs: [] }, /found no functions/],
    [{ total: 0 }, /no AST nodes/],
  ]) {
    f.update(changes)
    const result = f.run('--totals')
    assert.notEqual(result.status, 0)
    assert.match(result.stderr, message)
    assert.equal(result.stdout, '')
  }
})

test('parse-error tolerance accepts 0.5% and rejects larger fractions', t => {
  const f = fixture(t)
  f.update({ errors: 5 })
  succeeded(f.run('--totals'))
  f.update({ errors: 6 })
  const result = f.run('--totals')
  assert.notEqual(result.status, 0)
  assert.match(result.stderr, /0.600% of AST nodes are parse errors/)
})

test('delta names growth, simplification, additions, removals and adoption', t => {
  const f = fixture(t)
  const row = (name, cognitive, sloc, extra = {}) => ({
    file: 'projects/service/lib/utils.ts',
    line: 1,
    name,
    cognitive,
    sloc,
    cyclomatic: 1,
    ...extra,
  })
  f.put(
    'base.json',
    JSON.stringify({
      totals: {
        functions: 5,
        cognitive: 64,
        cyclomatic: 5,
        sloc: 21,
        over25: 1,
      },
      functions: [
        row('up', 24, 5),
        row('down', 30, 10),
        row('gone', 7, 3),
        row('utility', 2, 2, { scopes: ['projects/one'] }),
        row('<anonymous>', 1, 1),
      ],
    }),
  )
  f.put(
    'head.json',
    JSON.stringify({
      totals: {
        functions: 5,
        cognitive: 42,
        cyclomatic: 5,
        sloc: 22,
        over25: 1,
      },
      functions: [
        row('up', 26, 5),
        row('down', 2, 10),
        row('utility', 2, 2, { scopes: ['projects/one', 'projects/two'] }),
        row('added', 11, 3, { callers: 1 }),
        row('<anonymous>', 1, 2),
      ],
    }),
  )
  const output = succeeded(
    spawnSync(
      process.execPath,
      [
        join(tools, 'delta.mjs'),
        join(f.root, 'base.json'),
        join(f.root, 'head.json'),
        '0123456789abcdef',
      ],
      { encoding: 'utf8', env },
    ),
  )
  assert.match(output, /^Complexity vs 0123456789\n/)
  assert.match(output, /cognitive\s+64 ->\s+42\s+-22/)
  assert.match(output, /new functions over cognitive 10 \(1 of 2 new\)/)
  assert.match(output, /cog 24 -> 26\s+up.*<-- now over 25/)
  assert.match(output, /cog 30 -> 2\s+down/)
  assert.match(output, /removed: 2 functions, 8 cognitive/)
  assert.match(output, /utility\s+\+projects\/two\s+\(now 2\)/)
  assert.match(output, /single-use helpers alongside their only caller: 1/)
})

test('verification rejects a stale last row even after an earlier matching row', t => {
  const f = fixture(t)
  const sandbox = join(f.root, 'tooling')
  mkdirSync(join(sandbox, 'bin'), { recursive: true })
  for (const name of ['census.mjs', 'verify.sh'])
    copyFileSync(join(tools, name), join(sandbox, name))
  copyFileSync(join(f.root, 'fake-bca.mjs'), join(sandbox, 'bin/bca'))
  copyFileSync(join(f.root, 'config.json'), join(sandbox, 'bin/config.json'))
  const log = join(f.root, 'history.tsv')
  writeFileSync(
    log,
    'commit\tdate\tfunctions\tcognitive\tcyclomatic\tsloc\tover25\nold\tdate\t3\t54\t6\t17\t1\n',
  )
  const run = () =>
    spawnSync('bash', [join(sandbox, 'verify.sh')], {
      cwd: f.root,
      env: { ...env, LOG: log },
      encoding: 'utf8',
    })
  assert.match(succeeded(run()), /last row describes this tree/)
  writeFileSync(log, readFileSync(log, 'utf8') + 'stale\tdate\t0\t0\t0\t0\t0\n')
  const stale = run()
  assert.notEqual(stale.status, 0)
  assert.match(stale.stderr, /last row.*does not describe this tree/)
})
