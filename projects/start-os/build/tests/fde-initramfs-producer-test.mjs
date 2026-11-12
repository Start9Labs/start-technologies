import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import {
  appendFile,
  cp,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  readlink,
  rm,
  stat,
  writeFile,
} from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { basename, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import test from 'node:test'

import {
  capturedStreams,
  instrumentProducer,
  runNativeProducer,
} from '../lib/scripts/fde-initramfs-producer.mjs'
import { verifyGeneratedArchive } from '../lib/scripts/fde-initramfs-archive.mjs'
import { withPrivateInitramfs } from '../lib/scripts/fde-initramfs-generation.mjs'

assert.ok(
  process.argv[2] && process.argv[3],
  'Provide the installed initramfs-tools tree and reviewed provider fixture',
)
const installed = resolve(process.argv[2])
const providers = resolve(process.argv[3])
const native = await readFile(join(installed, 'usr/sbin/mkinitramfs'))
const hooks = await readFile(
  join(installed, 'usr/share/initramfs-tools/hook-functions'),
)
const adapted = instrumentProducer(native, hooks)
const entry = ': "${FDE_PREPARE:?Missing FDE preparation program}"'
const packaging = adapted.slice(adapted.indexOf(entry))

async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), 'fde-producer-'))
  t.after(() => rm(root, { recursive: true, force: true }))
  const tree = join(root, 'tree')
  const captures = join(root, 'captures')
  await mkdir(tree)
  await mkdir(captures, { mode: 0o700 })
  await writeFile(join(tree, 'payload'), 'native packaging data')
  await writeFile(join(root, 'early'), '')
  await writeFile(join(root, 'overlay'), '')
  return {
    root,
    tree,
    captures,
    archive: join(root, 'initrd'),
    environment: {
      ...process.env,
      DESTDIR: tree,
      outfile: join(root, 'initrd'),
      verbose: 'n',
      compress: 'gzip -n',
      SOURCE_DATE_EPOCH: '1',
      cpio_owner_root: '',
      cpio_reproducible: '',
      FDE_PREPARE: '/bin/true',
      FDE_CAPTURE_DIR: captures,
      __TMPEARLYCPIO: join(root, 'early'),
      __TMPCPIOGZ: join(root, 'overlay'),
      __TMPMAINFILES: join(root, 'main-list'),
      __TMPUNCOMPRESSEDFILES: join(root, 'uncompressed-list'),
    },
  }
}

function runPackaging(f, environment = {}) {
  return spawnSync('sh', ['-c', packaging], {
    env: { ...f.environment, ...environment },
    encoding: 'utf8',
  })
}

async function callback(f, source) {
  const path = join(f.root, 'prepare')
  await writeFile(path, '#!/bin/sh\nset -e\n' + source, { mode: 0o700 })
  return path
}

async function verify(f) {
  const streams = capturedStreams(f.captures)
  await verifyGeneratedArchive(f.archive, streams.prefixes, streams.mainCpio)
  for (const path of [...streams.prefixes, streams.mainCpio])
    assert.equal((await stat(path)).mode & 0o777, 0o600)
}

test('only reviewed providers are adapted and native ordering stays before preparation', () => {
  assert.throws(
    () => instrumentProducer(Buffer.concat([native, Buffer.from('\n')]), hooks),
    /mkinitramfs/,
  )
  assert.throws(
    () => instrumentProducer(native, Buffer.concat([hooks, Buffer.from('\n')])),
    /hook-functions/,
  )
  assert.ok(
    adapted.indexOf('cache_run_scripts "${DESTDIR}"') < adapted.indexOf(entry),
  )
  assert.ok(adapted.indexOf('ldconfig -r') < adapted.indexOf(entry))
  assert.ok(adapted.indexOf('copy_file DSDT') < adapted.indexOf(entry))
  assert.ok(adapted.indexOf(entry) < adapted.indexOf('find . \\( ! -type d'))
  const syntax = spawnSync('sh', ['-n'], { input: adapted, encoding: 'utf8' })
  assert.equal(syntax.status, 0, syntax.stderr)
})

test('native packaging captures a gzip generation without mutating provider sources', async t => {
  const f = await fixture(t)
  const result = runPackaging(f)
  assert.equal(result.status, 0, result.stderr)
  await verify(f)
  assert.deepEqual(
    await readFile(join(installed, 'usr/sbin/mkinitramfs')),
    native,
  )
  assert.deepEqual(
    await readFile(join(installed, 'usr/share/initramfs-tools/hook-functions')),
    hooks,
  )
  const archive = await readFile(f.archive)
  const second = runPackaging(f)
  assert.notEqual(second.status, 0)
  assert.deepEqual(await readFile(f.archive), archive)
})

test('native early and precompressed-file streams are preserved as separate prefixes', async t => {
  const f = await fixture(t)
  await writeFile(
    join(f.tree, 'data.xz'),
    'opaque data with a native split-file suffix',
  )
  const prefix = spawnSync('cpio', ['--quiet', '-o', '-H', 'newc'], {
    cwd: f.tree,
    input: 'payload\n',
  })
  assert.equal(prefix.status, 0, prefix.stderr?.toString())
  await writeFile(f.environment.__TMPEARLYCPIO, prefix.stdout)
  const result = runPackaging(f)
  assert.equal(result.status, 0, result.stderr)
  await verify(f)
  const streams = capturedStreams(f.captures)
  assert.deepEqual(await readFile(streams.prefixes[0]), prefix.stdout)
  assert.ok((await stat(streams.prefixes[1])).size > 0)
})

test('the preparation boundary can harden the completed native provider tree before capture', async t => {
  const f = await fixture(t)
  await rm(f.tree, { recursive: true })
  await cp(providers, f.tree, { recursive: true, verbatimSymlinks: true })
  const result = runPackaging(f, {
    FDE_PREPARE: fileURLToPath(
      new URL('../lib/scripts/fde-harden-initramfs.mjs', import.meta.url),
    ),
  })
  assert.equal(result.status, 0, result.stderr)
  await verify(f)
  assert.ok(
    (await readFile(join(f.tree, 'init'), 'utf8')).includes(
      "trap 'fde_fail' EXIT",
    ),
  )
})

test('a failed preparation program cannot reach native packaging', async t => {
  const f = await fixture(t)
  const result = runPackaging(f, { FDE_PREPARE: '/bin/false' })
  assert.notEqual(result.status, 0)
  await assert.rejects(stat(f.archive), { code: 'ENOENT' })
})

test('unsupported compression and appended native overlays cannot be packaged', async t => {
  const f = await fixture(t)
  const compression = runPackaging(f, { compress: 'zstd -q' })
  assert.notEqual(compression.status, 0)
  assert.match(compression.stderr, /requires gzip/)
  await writeFile(f.environment.__TMPCPIOGZ, 'unreviewed appended stream')
  const overlay = runPackaging(f)
  assert.notEqual(overlay.status, 0)
  assert.match(overlay.stderr, /Unsupported appended/)
  await assert.rejects(stat(f.archive), { code: 'ENOENT' })
})

test('capture storage cannot be placed inside the generated tree', async t => {
  const f = await fixture(t)
  const inside = join(f.tree, 'captures')
  await mkdir(inside)
  const result = runPackaging(f, { FDE_CAPTURE_DIR: inside })
  assert.notEqual(result.status, 0)
  assert.match(result.stderr, /inside generated tree/)
  assert.deepEqual(await readdir(inside), [])
})

test('one private generation owns native capture, verification, consumption and cleanup', async t => {
  const f = await fixture(t)
  let workspace
  const value = await withPrivateInitramfs(
    f.root,
    async generation => {
      workspace = generation.directory
      assert.equal((await stat(workspace)).mode & 0o777, 0o700)
      assert.equal((await stat(generation.captures)).mode & 0o777, 0o700)
      const result = runPackaging(f, {
        outfile: generation.archive,
        FDE_CAPTURE_DIR: generation.captures,
      })
      assert.equal(result.status, 0, result.stderr)
    },
    async archive => {
      assert.equal(archive, join(workspace, 'initrd'))
      assert.ok((await stat(archive)).size > 0)
      return 'consumed'
    },
  )
  assert.equal(value, 'consumed')
  await assert.rejects(stat(workspace), { code: 'ENOENT' })
  assert.equal(
    await readFile(join(f.tree, 'payload'), 'utf8'),
    'native packaging data',
  )
})

for (const failure of ['producer', 'verification', 'consumer']) {
  test(`private generation cleans up after ${failure} failure`, async t => {
    const f = await fixture(t)
    let workspace
    let consumed = false
    await assert.rejects(
      withPrivateInitramfs(
        f.root,
        async generation => {
          workspace = generation.directory
          const result = runPackaging(f, {
            outfile: generation.archive,
            FDE_CAPTURE_DIR: generation.captures,
            ...(failure === 'producer' ? { FDE_PREPARE: '/bin/false' } : {}),
          })
          if (failure === 'producer') {
            assert.notEqual(result.status, 0)
            throw Error('producer rejected')
          }
          assert.equal(result.status, 0, result.stderr)
          if (failure === 'verification')
            await appendFile(
              capturedStreams(generation.captures).mainCpio,
              'wrong private capture',
            )
        },
        async archive => {
          consumed = true
          assert.ok((await stat(archive)).size > 0)
          throw Error('consumer rejected')
        },
      ),
      failure === 'verification'
        ? /main CPIO stream differs/
        : new RegExp(`${failure} rejected`),
    )
    assert.equal(consumed, failure === 'consumer')
    await assert.rejects(stat(workspace), { code: 'ENOENT' })
    assert.equal(
      await readFile(join(f.tree, 'payload'), 'utf8'),
      'native packaging data',
    )
  })
}

for (const fail of [false, true]) {
  test(
    `native producer namespace ends detached writers before ${fail ? 'failure cleanup' : 'archive consumption'}`,
    { skip: process.env.FDE_KERNEL_PROCESS_TEST !== '1' },
    async t => {
      const f = await fixture(t)
      const parentNamespace = await readlink('/proc/self/ns/pid')
      let workspace
      let consumed = false
      const namespaceFile = join(f.root, 'writer-namespace')
      const prepare = await callback(
        f,
        `setsid sh -c 'readlink /proc/self/ns/pid > "$FDE_TEST_NAMESPACE"; while :; do printf x >> "$FDE_TEST_WRITER"; sleep 0.01; done' </dev/null >/dev/null 2>&1 &
while [ ! -s "$FDE_TEST_NAMESPACE" ] || [ ! -s "$FDE_TEST_WRITER" ]; do sleep 0.01; done
exit ${fail ? 17 : 0}
`,
      )
      const producer = join(f.root, 'producer')
      await writeFile(producer, '#!/bin/sh\n' + packaging, { mode: 0o700 })
      async function assertWritersEnded() {
        const namespace = (await readFile(namespaceFile, 'utf8')).trim()
        assert.notEqual(namespace, parentNamespace)
        for (const pid of await readdir('/proc')) {
          if (!/^\d+$/.test(pid)) continue
          try {
            assert.notEqual(await readlink(`/proc/${pid}/ns/pid`), namespace)
          } catch (error) {
            if (error.code !== 'ENOENT' && error.code !== 'ESRCH') throw error
          }
        }
        assert.equal(await readlink('/proc/self/ns/pid'), parentNamespace)
      }
      const operation = withPrivateInitramfs(
        f.root,
        async generation => {
          workspace = generation.directory
          try {
            await runNativeProducer(producer, [], {
              ...f.environment,
              outfile: generation.archive,
              FDE_CAPTURE_DIR: generation.captures,
              FDE_PREPARE: prepare,
              FDE_TEST_NAMESPACE: namespaceFile,
              FDE_TEST_WRITER: join(workspace, 'writer'),
            })
          } catch (error) {
            await assertWritersEnded()
            throw error
          }
          await assertWritersEnded()
        },
        async archive => {
          consumed = true
          await assertWritersEnded()
          assert.ok((await stat(archive)).size > 0)
        },
      )
      if (fail) await assert.rejects(operation, /Native producer exited with 1/)
      else await operation
      assert.equal(consumed, !fail)
      await assert.rejects(stat(workspace), { code: 'ENOENT' })
      await assertWritersEnded()
    },
  )
}

for (const name of ['mainCpio', 'uncompressed']) {
  test(`native ${name} capture I/O failure survives the packaging pipelines`, async t => {
    const f = await fixture(t)
    await writeFile(join(f.tree, 'data.xz'), 'opaque native split-file data')
    const streams = capturedStreams(f.captures)
    const filename = basename(
      name === 'mainCpio' ? streams.mainCpio : streams.prefixes[1],
    )
    const program = await callback(
      f,
      `rm -- "$FDE_CAPTURE_DIR/${filename}"\nmkdir -- "$FDE_CAPTURE_DIR/${filename}"\n`,
    )
    const result = runPackaging(f, { FDE_PREPARE: program })
    assert.notEqual(result.status, 0)
    assert.match(
      result.stderr,
      name === 'mainCpio'
        ? /main capture failed/
        : /uncompressed capture failed/,
    )
  })
}
