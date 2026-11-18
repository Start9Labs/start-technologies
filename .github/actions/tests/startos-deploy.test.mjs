import assert from 'node:assert/strict'
import { chmodSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import yaml from 'js-yaml'
import { REPO, fixture } from '../../../scripts/tests/fixtures.mjs'

const load = name =>
  yaml.load(readFileSync(join(REPO, '.github/workflows', name), 'utf8'))
const iso = load('startos-iso.yaml')
const leg = load('startos-image.yaml').jobs.deploy
const step = (job, name) => {
  const found = job.steps.find(step => step.name === name)
  assert.ok(found, name)
  return found.run
}

const VERSION = '0.4.1'
const SHA = 'a'.repeat(40)
const OTHER = 'b'.repeat(40)
const PLATFORM = 'x86_64'
const FILES = [
  `startos-${VERSION}-aaaaaaa_${PLATFORM}.squashfs`,
  `startos-${VERSION}-aaaaaaa_${PLATFORM}.iso`,
]

// A registry index in `start-cli registry os index` form.
const index = versions => ({ versions })
const entry = (gitHash, assets = {}) => ({
  gitHash,
  iso: {},
  img: {},
  squashfs: {},
  ...assets,
})

// Fakes `start-cli` against a JSON registry and `s3cmd` against a directory bucket.
function deployFixture(t, registry) {
  const f = fixture(t)
  f.write('registry.json', JSON.stringify(registry))
  f.write('cli.log', '')
  f.write('add-failures', '0')
  f.nodeExecutable(
    'start-cli',
    `import { appendFileSync, mkdirSync, readFileSync, rmdirSync, writeFileSync } from 'node:fs'
const root = ${JSON.stringify(f.root)}
const args = process.argv.slice(2).filter(arg => !arg.startsWith('--registry='))
const command = args.slice(0, 4).join(' ')
const option = name => args.find(arg => arg.startsWith('--' + name + '='))?.split('=')[1]
// Legs index their files concurrently.
const lock = root + '/registry.lock'
for (;;) {
  try { mkdirSync(lock); break } catch { Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 5) }
}
let status = 0
try {
  appendFileSync(root + '/cli.log', JSON.stringify(args) + '\\n')
  const state = JSON.parse(readFileSync(root + '/registry.json', 'utf8'))
  const save = () => writeFileSync(root + '/registry.json', JSON.stringify(state))
  if (command.startsWith('registry os index')) {
    if (process.env.INDEX_FAILS) status = 1
    else process.stdout.write(JSON.stringify(state))
  } else if (command === 'registry os version remove') {
    delete state.versions[args[4]]
    save()
  } else if (command === 'registry os version add') {
    const hash = args.indexOf('--git-hash')
    const [version] = args
      .slice(4)
      .filter((arg, i, all) => !arg.startsWith('--') && all[i - 1] !== '--git-hash')
    state.versions[version] = {
      ...(state.versions[version] ?? { iso: {}, img: {}, squashfs: {} }),
      gitHash: hash === -1 ? null : args[hash + 1],
      security: args.includes('--security'),
    }
    save()
  } else if (command === 'registry os asset add') {
    const failures = Number(readFileSync(root + '/add-failures', 'utf8'))
    const version = state.versions[option('version')]
    const [file, url] = args.slice(4).filter(arg => !arg.startsWith('--'))
    const kind = file.split('.').pop()
    const platform = option('platform')
    if (version && process.env.PROMOTED_DURING_ADD) {
      // Another publisher replaces the entry, images included, while this leg indexes.
      version.gitHash = process.env.PROMOTED_DURING_ADD
      for (const slot of ['squashfs', 'iso'])
        version[slot][platform] = { urls: ['https://cdn.example/promoted.' + slot] }
      save()
    }
    // A slot holding other bytes rejects the write, like a commitment mismatch.
    const held = version?.[kind]?.[platform]?.urls
    if (failures > 0) {
      writeFileSync(root + '/add-failures', String(failures - 1))
      status = 1
    } else if (!version || (held && !held.includes(url))) {
      status = 1
    } else {
      version[kind][platform] = { urls: [url] }
      if (process.env.REREGISTER_DURING_ADD) version.gitHash = process.env.REREGISTER_DURING_ADD
      save()
    }
  } else if (command === 'registry os asset remove') {
    const [kind, version, platform] = args.slice(4)
    if (state.versions[version]) {
      delete state.versions[version][kind][platform]
      save()
    } else status = 1
  } else status = 2
} finally {
  rmdirSync(lock)
}
process.exit(status)
`,
  )
  f.nodeExecutable(
    's3cmd',
    `import { appendFileSync, copyFileSync, existsSync, readFileSync } from 'node:fs'
import { createHash } from 'node:crypto'
const root = ${JSON.stringify(f.root)}
const [command, ...rest] = process.argv.slice(2)
const object = key => root + '/bucket/' + encodeURIComponent(key)
if (command === 'ls') {
  const key = rest.at(-1)
  if (existsSync(object(key))) console.log('2026-11-16 00:00  1  ' + key)
} else if (command === 'info') {
  const key = rest.at(-1)
  if (!existsSync(object(key))) process.exit(12)
  console.log('   MD5 sum:   ' + createHash('md5').update(readFileSync(object(key))).digest('hex'))
} else if (command === 'put') {
  const [file, key] = rest.filter(arg => arg !== '-P')
  appendFileSync(root + '/puts.log', key + '\\n')
  copyFileSync(file, object(key))
} else {
  process.exit(2)
}
`,
  )
  f.nodeExecutable('sleep', '')
  chmodSync(
    f.write(
      'scripts/s3-put-once.sh',
      readFileSync(join(REPO, 'scripts/s3-put-once.sh')),
    ),
    0o755,
  )
  f.write('bucket/.keep', '')
  f.write('puts.log', '')
  for (const file of FILES) f.write(`artifacts/${file}`, `${file} built\n`)
  const env = {
    ...f.env,
    PATH: `${f.root}:${f.env.PATH}`,
    GITHUB_WORKSPACE: f.root,
    VERSION,
    PLATFORM,
    REGISTRY: 'https://registry.example',
    COMMIT: SHA,
    S3_BUCKET: 's3://bucket',
    S3_CDN: 'https://cdn.example',
    FORCE_OVERWRITE: 'false',
  }
  return {
    ...f,
    run: (script, extra = {}, cwd = f.root) =>
      f.run('bash', ['-e', '-c', script], {
        cwd,
        env: { ...env, ...extra },
        encoding: 'utf8',
      }),
    registry: () => JSON.parse(readFileSync(join(f.root, 'registry.json'))),
    calls: () =>
      readFileSync(join(f.root, 'cli.log'), 'utf8')
        .trim()
        .split('\n')
        .filter(Boolean)
        .map(line => JSON.parse(line)),
    puts: () =>
      readFileSync(join(f.root, 'puts.log'), 'utf8')
        .trim()
        .split('\n')
        .filter(Boolean),
    key: file => `s3://bucket/v${VERSION}/${file}`,
    object: (file, content) =>
      f.write(
        `bucket/${encodeURIComponent(`s3://bucket/v${VERSION}/${file}`)}`,
        content,
      ),
  }
}

const register = step(iso.jobs['deploy-prepare'], 'Register OS version')

for (const [name, registered, wiped] of [
  ['an entry for another commit is removed with its assets', OTHER, true],
  ['an entry without a recorded commit is removed', null, true],
  ['an entry for this commit keeps its assets', SHA, false],
]) {
  test(`deploy-prepare: ${name}`, t => {
    const assets = {
      iso: { [PLATFORM]: { urls: ['https://cdn.example/old.iso'] } },
    }
    const f = deployFixture(t, index({ [VERSION]: entry(registered, assets) }))
    f.write(
      'runner/release-notes.json',
      JSON.stringify({ releaseNotes: 'Notes.' }),
    )
    f.write('runner/release-security', 'false\n')
    const result = f.run(register, { RUNNER_TEMP: join(f.root, 'runner') })
    assert.equal(result.status, 0, result.stderr)
    const removed = f
      .calls()
      .some(call => call.slice(0, 4).join(' ') === 'registry os version remove')
    assert.equal(removed, wiped)
    const version = f.registry().versions[VERSION]
    assert.equal(version.gitHash, SHA)
    assert.deepEqual(version.iso, wiped ? {} : assets.iso)
  })
}

test('deploy-prepare: an absent version is registered without a removal', t => {
  const f = deployFixture(t, index({}))
  f.write(
    'runner/release-notes.json',
    JSON.stringify({ releaseNotes: 'Notes.' }),
  )
  f.write('runner/release-security', 'true\n')
  const result = f.run(register, { RUNNER_TEMP: join(f.root, 'runner') })
  assert.equal(result.status, 0, result.stderr)
  assert.deepEqual(
    f.calls().map(call => call.slice(0, 4).join(' ')),
    ['registry os index', 'registry os version add'],
  )
  assert.deepEqual(f.registry().versions[VERSION].gitHash, SHA)
  assert.equal(f.registry().versions[VERSION].security, true)
})

test('deploy-prepare: an unreadable index stops before any write', t => {
  const f = deployFixture(t, index({ [VERSION]: entry(OTHER) }))
  f.write(
    'runner/release-notes.json',
    JSON.stringify({ releaseNotes: 'Notes.' }),
  )
  f.write('runner/release-security', 'false\n')
  const result = f.run(register, {
    RUNNER_TEMP: join(f.root, 'runner'),
    INDEX_FAILS: '1',
  })
  assert.notEqual(result.status, 0)
  assert.deepEqual(f.calls(), [['registry', 'os', 'index']])
  assert.equal(f.registry().versions[VERSION].gitHash, OTHER)
})

const upload = step(leg, 'Upload to S3')

for (const [name, existing, force, status, uploaded] of [
  ['uploads images the bucket lacks', null, false, 0, FILES],
  ['skips objects that already hold these bytes', 'same', false, 0, [FILES[1]]],
  ['refuses to replace different bytes', 'different', false, 1, [FILES[1]]],
  [
    'replaces different bytes under force-overwrite',
    'different',
    true,
    0,
    FILES,
  ],
]) {
  test(`deploy leg upload ${name}`, t => {
    const f = deployFixture(t, index({}))
    if (existing)
      f.object(
        FILES[0],
        existing === 'same' ? `${FILES[0]} built\n` : 'another build\n',
      )
    const result = f.run(
      upload,
      { FORCE_OVERWRITE: String(force) },
      join(f.root, 'artifacts'),
    )
    assert.equal(result.status, status, result.stdout + result.stderr)
    assert.deepEqual(f.puts().sort(), uploaded.map(f.key).sort())
  })
}

const wrtUpload = step(
  load('start-wrt.yaml').jobs.deploy,
  'Upload images to S3',
)
const WRT_IMAGES = [
  'startwrt-1.3.0-aaaaaaa_spacemit-k1-sdcard.img.gz',
  'startwrt-1.3.0-aaaaaaa_spacemit-k1-sysupgrade.img.gz',
]

test('start-wrt deploy upload skips images an earlier attempt uploaded and refuses different bytes', t => {
  const f = deployFixture(t, index({}))
  for (const image of WRT_IMAGES)
    f.write(`artifacts/${image}`, `${image} built\n`)
  f.object(WRT_IMAGES[0], `${WRT_IMAGES[0]} built\n`)
  const rerun = f.run(wrtUpload)
  assert.equal(rerun.status, 0, rerun.stdout + rerun.stderr)
  assert.deepEqual(f.puts(), [f.key(WRT_IMAGES[1])])
  f.object(WRT_IMAGES[1], 'another build\n')
  const conflict = f.run(wrtUpload)
  assert.equal(conflict.status, 1, conflict.stdout + conflict.stderr)
  assert.deepEqual(f.puts(), [f.key(WRT_IMAGES[1])])
})

const indexAssets = step(leg, 'Index assets in registry')
const indexed = registry =>
  Object.fromEntries(
    ['squashfs', 'iso'].map(kind => [
      kind,
      registry.versions[VERSION]?.[kind]?.[PLATFORM]?.urls,
    ]),
  )
const urls = {
  squashfs: [`https://cdn.example/v${VERSION}/${FILES[0]}`],
  iso: [`https://cdn.example/v${VERSION}/${FILES[1]}`],
}

test('deploy leg indexes its images under the commit deploy-prepare registered', t => {
  const f = deployFixture(t, index({ [VERSION]: entry(SHA) }))
  f.write('add-failures', '1')
  const result = f.run(indexAssets, {}, join(f.root, 'artifacts'))
  assert.equal(result.status, 0, result.stdout + result.stderr)
  assert.deepEqual(indexed(f.registry()), urls)
})

test('deploy leg leaves another commit’s entry untouched', t => {
  const f = deployFixture(t, index({ [VERSION]: entry(OTHER) }))
  const result = f.run(indexAssets, {}, join(f.root, 'artifacts'))
  assert.equal(result.status, 1)
  assert.deepEqual(f.calls(), [['registry', 'os', 'index']])
})

test('deploy leg withdraws its images when the version is re-registered mid-index', t => {
  const f = deployFixture(t, index({ [VERSION]: entry(SHA) }))
  const result = f.run(
    indexAssets,
    { REREGISTER_DURING_ADD: OTHER },
    join(f.root, 'artifacts'),
  )
  assert.equal(result.status, 1)
  assert.deepEqual(indexed(f.registry()), {
    squashfs: undefined,
    iso: undefined,
  })
  assert.equal(f.registry().versions[VERSION].gitHash, OTHER)
})

test('deploy leg leaves another publisher’s images in place when the entry changes mid-index', t => {
  const f = deployFixture(t, index({ [VERSION]: entry(SHA) }))
  const result = f.run(
    indexAssets,
    { PROMOTED_DURING_ADD: OTHER },
    join(f.root, 'artifacts'),
  )
  assert.equal(result.status, 1)
  assert.deepEqual(indexed(f.registry()), {
    squashfs: ['https://cdn.example/promoted.squashfs'],
    iso: ['https://cdn.example/promoted.iso'],
  })
  assert.equal(
    f
      .calls()
      .some(call => call.slice(0, 4).join(' ') === 'registry os asset remove'),
    false,
  )
})

test('deploy leg replaces its own commitments under force-overwrite', t => {
  const f = deployFixture(
    t,
    index({
      [VERSION]: entry(SHA, {
        iso: { [PLATFORM]: { urls: ['https://cdn.example/stale.iso'] } },
      }),
    }),
  )
  const result = f.run(
    indexAssets,
    { FORCE_OVERWRITE: 'true' },
    join(f.root, 'artifacts'),
  )
  assert.equal(result.status, 0, result.stdout + result.stderr)
  assert.deepEqual(indexed(f.registry()), urls)
})

const prune = step(
  iso.jobs['deploy-prune'],
  'Prune superseded images',
).replaceAll('${{ env.S3_BUCKET }}', 's3://bucket')
const pruneEnv = iso.jobs['deploy-prune'].steps.find(
  step => step.name === 'Prune superseded images',
).env

function pruneFixture(t, deployed, published) {
  const f = deployFixture(t, index({}))
  const referenced = {
    result: {
      versions: {
        [VERSION]: entry(deployed, {
          squashfs: {
            [PLATFORM]: {
              urls: [`https://cdn.example/v${VERSION}/${FILES[0]}`],
            },
          },
        }),
      },
    },
  }
  f.write('os-index.json', JSON.stringify(referenced))
  f.nodeExecutable(
    'curl',
    `import { copyFileSync } from 'node:fs'
const args = process.argv.slice(2)
copyFileSync(${JSON.stringify(join(f.root, 'os-index.json'))}, args[args.indexOf('-o') + 1])
`,
  )
  f.write(
    's3cmd.mjs',
    `#!/usr/bin/env node
import { appendFileSync } from 'node:fs'
const [command] = process.argv.slice(2)
if (command === 'ls') console.log('2026-01-01 00:00  1  s3://bucket/v${VERSION}/startos-${VERSION}-ccccccc_${PLATFORM}.iso')
else appendFileSync(${JSON.stringify(join(f.root, 'deleted.log'))}, process.argv.slice(3).join(' ') + '\\n')
`,
  )
  if (published) f.write(`published/published-${PLATFORM}.txt`, published)
  return f
}

for (const [name, deployed, published, status, warning] of [
  [
    'skips once another commit owns the version',
    OTHER,
    `${FILES[1]}\n`,
    0,
    /now registered for b{40}/,
  ],
  ['skips when no platform published', SHA, null, 0, /no platform published/],
  [
    'fails when this run’s images are missing from the index',
    SHA,
    `${FILES[1]}\n`,
    1,
    /missing from the registries/,
  ],
]) {
  test(`deploy-prune ${name}`, t => {
    const f = pruneFixture(t, deployed, published)
    const result = f.run(prune, {
      PUBLISHED_LISTS: 'success',
      PRUNE_REGISTRIES: 'https://registry.example',
      MIN_AGE_DAYS: String(pruneEnv.MIN_AGE_DAYS),
    })
    assert.equal(result.status, status, result.stdout + result.stderr)
    assert.match(result.stdout, warning)
    assert.equal(f.run('test -e deleted.log').status, 1)
  })
}

test('a deploy leg missing one of its images fails before uploading', t => {
  const check = step(leg, 'Check downloaded images')
  for (const [platform, files, ok] of [
    [PLATFORM, FILES, true],
    [PLATFORM, [FILES[0]], false],
    [
      PLATFORM,
      [`startos-${VERSION}-aaaaaaa_${PLATFORM}-nonfree.iso`, FILES[0]],
      false,
    ],
    [
      'raspberrypi',
      [
        `startos-${VERSION}-aaaaaaa_raspberrypi.squashfs`,
        `startos-${VERSION}-aaaaaaa_raspberrypi.img.gz`,
      ],
      true,
    ],
    [
      'raspberrypi',
      [
        `startos-${VERSION}-aaaaaaa_raspberrypi.squashfs`,
        `startos-${VERSION}-aaaaaaa_raspberrypi.img`,
      ],
      false,
    ],
    ['raspberrypi', [`startos-${VERSION}-aaaaaaa_raspberrypi.squashfs`], false],
  ]) {
    const f = fixture(t)
    for (const file of files) f.write(`artifacts/${file}`, '')
    const result = f.run('bash', ['-e', '-c', check], {
      cwd: join(f.root, 'artifacts'),
      env: { ...f.env, PLATFORM: platform },
      encoding: 'utf8',
    })
    assert.equal(
      result.status === 0,
      ok,
      `${platform} ${files}: ${result.stdout}`,
    )
  }
})
