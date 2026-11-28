import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'

import { verifyGeneratedArchive } from '../lib/scripts/fde-initramfs-archive.mjs'

function run(program, args, options) {
  const result = spawnSync(program, args, options)
  assert.equal(
    result.status,
    0,
    result.error?.message ?? result.stderr?.toString(),
  )
  return result.stdout
}

function rejectsTrailingInput(error) {
  return (
    /unconsumed compressed input/.test(error.message) ||
    error.code === 'ABORT_ERR' ||
    error.code === 'ERR_STREAM_PREMATURE_CLOSE'
  )
}

function compress(bytes) {
  return run('gzip', ['--no-name', '--stdout'], { input: bytes })
}

async function nativeCpio(root, name, filename, content) {
  const source = join(root, name)
  await mkdir(source)
  await writeFile(join(source, filename), content)
  return run('cpio', ['--create', '--format=newc', '--quiet'], {
    cwd: source,
    input: filename + '\n',
  })
}

async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), 'fde-archive-'))
  t.after(() => rm(root, { recursive: true, force: true }))
  const main = await nativeCpio(
    root,
    'main',
    'payload',
    'trusted configuration',
  )
  const other = await nativeCpio(
    root,
    'other',
    'unreviewed-hook',
    'untrusted addition',
  )
  const first = await nativeCpio(
    root,
    'first',
    'first-data',
    'first native prefix',
  )
  const second = await nativeCpio(
    root,
    'second',
    'second-data',
    'second native prefix',
  )
  const mainCpio = join(root, 'main.cpio')
  const archive = join(root, 'initrd')
  const prefixes = [
    join(root, 'first.cpio'),
    join(root, 'empty'),
    join(root, 'second.cpio'),
  ]
  await writeFile(mainCpio, main)
  await writeFile(prefixes[0], first)
  await writeFile(prefixes[1], '')
  await writeFile(prefixes[2], second)
  return { root, main, other, first, second, mainCpio, archive, prefixes }
}

test('native gzip/CPIO roundtrip without prefixes preserves its sources', async t => {
  const f = await fixture(t)
  const compressed = compress(f.main)
  await writeFile(f.archive, compressed)
  await verifyGeneratedArchive(f.archive, [], f.mainCpio)
  assert.deepEqual(await readFile(f.mainCpio), f.main)
  assert.deepEqual(await readFile(f.archive), compressed)
})

test('all native prefixes, including empty ones, are checked in order', async t => {
  const f = await fixture(t)
  await writeFile(
    f.archive,
    Buffer.concat([f.first, f.second, compress(f.main)]),
  )
  await verifyGeneratedArchive(f.archive, f.prefixes, f.mainCpio)
  await assert.rejects(
    verifyGeneratedArchive(f.archive, [...f.prefixes].reverse(), f.mainCpio),
    /prefix differs/,
  )
})

test('an appended compressed CPIO missed by native listing is rejected', async t => {
  const f = await fixture(t)
  await writeFile(
    f.archive,
    Buffer.concat([compress(f.main), compress(f.other)]),
  )
  const listing = run('unmkinitramfs', ['--list', f.archive], {
    encoding: 'utf8',
  })
  assert.ok(listing.includes('payload'))
  assert.ok(!listing.includes('unreviewed-hook'))
  await assert.rejects(
    verifyGeneratedArchive(f.archive, [], f.mainCpio),
    /main CPIO stream differs/,
  )
})

for (const [name, bytes, prefixes, error] of [
  [
    'changed main CPIO',
    f => compress(f.other),
    false,
    /main CPIO stream differs/,
  ],
  ['uncompressed tail', f => Buffer.concat([compress(f.main), f.other]), false],
  [
    'garbage tail',
    f =>
      Buffer.concat([compress(f.main), Buffer.from('not another gzip member')]),
    false,
  ],
  [
    'zero padding after gzip',
    f => Buffer.concat([compress(f.main), Buffer.alloc(4)]),
    false,
    rejectsTrailingInput,
  ],
  [
    'compressed CPIO hidden after zero padding',
    f => Buffer.concat([compress(f.main), Buffer.alloc(4), compress(f.other)]),
    false,
    rejectsTrailingInput,
  ],
  [
    'uncompressed CPIO hidden after zero padding',
    f => Buffer.concat([compress(f.main), Buffer.alloc(4), f.other]),
    false,
    rejectsTrailingInput,
  ],
  [
    'a tail across input chunks',
    f =>
      Buffer.concat([
        compress(f.main),
        Buffer.alloc(128 * 1024),
        compress(f.other),
      ]),
    false,
  ],
  ['truncated gzip', f => compress(f.main).subarray(0, -5), false],
  [
    'invalid gzip checksum',
    f => {
      const data = compress(f.main)
      data[data.length - 8] ^= 1
      return data
    },
    false,
  ],
  [
    'changed prefix',
    f => {
      const data = Buffer.from(f.first)
      data[0] ^= 1
      return Buffer.concat([data, f.second, compress(f.main)])
    },
    true,
    /prefix differs/,
  ],
  ['truncated prefix', f => f.first.subarray(0, -1), true, /prefix differs/],
  [
    'appended compressed CPIO after prefixes',
    f =>
      Buffer.concat([f.first, f.second, compress(f.main), compress(f.other)]),
    true,
    /main CPIO stream differs/,
  ],
]) {
  test(`${name} cannot pass provenance verification`, async t => {
    const f = await fixture(t)
    await writeFile(f.archive, bytes(f))
    await assert.rejects(
      verifyGeneratedArchive(f.archive, prefixes ? f.prefixes : [], f.mainCpio),
      error,
    )
  })
}
