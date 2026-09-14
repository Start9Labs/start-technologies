import assert from 'node:assert/strict'
import {
  mkdir,
  mkdtemp,
  readFile,
  rm,
  symlink,
  writeFile,
} from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import test from 'node:test'

import {
  collectImmutableAssets,
  generateImmutableAssets,
  writeEmptyImmutableAssets,
} from './generate-immutable-assets.mjs'

async function fixture(t) {
  const root = await mkdtemp(path.join(os.tmpdir(), 'immutable-assets-'))
  const browserRoot = path.join(root, 'browser')
  const statsPath = path.join(root, 'stats.json')
  await mkdir(browserRoot)
  t.after(() => rm(root, { force: true, recursive: true }))
  return { browserRoot, root, statsPath }
}

async function writeStats(statsPath, outputs) {
  await writeFile(
    statsPath,
    JSON.stringify({
      outputs: Object.fromEntries(outputs.map(output => [output, {}])),
    }),
  )
}

test('writes exact nested output keys in deterministic order and deletes stats', async t => {
  const { browserRoot, statsPath } = await fixture(t)
  await mkdir(path.join(browserRoot, 'chunks'))
  await writeFile(path.join(browserRoot, 'z.js'), '')
  await writeFile(path.join(browserRoot, 'chunks', 'main.exact.js'), '')
  await writeStats(statsPath, ['z.js', 'chunks/main.exact.js'])

  await generateImmutableAssets(statsPath, browserRoot)

  assert.equal(
    await readFile(path.join(browserRoot, 'immutable-assets.txt'), 'utf8'),
    'chunks/main.exact.js\nz.js\n',
  )
  await assert.rejects(readFile(statsPath), { code: 'ENOENT' })
})

test('skips virtual, missing, and symlinked outputs', async t => {
  const { browserRoot, root, statsPath } = await fixture(t)
  await writeFile(path.join(browserRoot, 'main.js'), '')
  await writeFile(path.join(root, 'outside.js'), '')
  await symlink(
    path.join(root, 'outside.js'),
    path.join(browserRoot, 'linked.js'),
  )
  await writeStats(statsPath, [
    'component.virtual.css',
    'missing.js',
    'linked.js',
    'main.js',
  ])

  await generateImmutableAssets(statsPath, browserRoot)

  assert.equal(
    await readFile(path.join(browserRoot, 'immutable-assets.txt'), 'utf8'),
    'main.js\n',
  )
})

test('sorts and deduplicates collected outputs', async t => {
  const { browserRoot } = await fixture(t)
  await writeFile(path.join(browserRoot, 'a.js'), '')
  await writeFile(path.join(browserRoot, 'b.js'), '')

  assert.deepEqual(
    await collectImmutableAssets(['b.js', 'a.js', 'b.js'], browserRoot),
    ['a.js', 'b.js'],
  )
})

for (const unsafePath of [
  '/absolute.js',
  'C:/absolute.js',
  '../outside.js',
  'nested/../main.js',
  './main.js',
  'nested//main.js',
  'bad\\path.js',
  'bad\npath.js',
  'bad\rpath.js',
]) {
  test(`rejects unsafe output path ${JSON.stringify(unsafePath)}`, async t => {
    const { browserRoot, statsPath } = await fixture(t)
    await writeStats(statsPath, [unsafePath])

    await assert.rejects(
      generateImmutableAssets(statsPath, browserRoot),
      /output path/i,
    )
    assert.equal(
      await readFile(statsPath, 'utf8'),
      JSON.stringify({ outputs: { [unsafePath]: {} } }),
    )
  })
}

test('fails when no generated output is a physical file', async t => {
  const { browserRoot, statsPath } = await fixture(t)
  await writeStats(statsPath, ['component.virtual.css'])

  await assert.rejects(
    generateImmutableAssets(statsPath, browserRoot),
    /no physical generated outputs/,
  )
  await readFile(statsPath)
})

test('writes a one-newline development manifest', async t => {
  const { browserRoot } = await fixture(t)

  await writeEmptyImmutableAssets(browserRoot)

  assert.equal(
    await readFile(path.join(browserRoot, 'immutable-assets.txt'), 'utf8'),
    '\n',
  )
})
