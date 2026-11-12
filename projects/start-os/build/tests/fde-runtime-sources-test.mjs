import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import {
  cp,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rename,
  rm,
  symlink,
  writeFile,
} from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import test from 'node:test'

import { admitRuntimeSources } from '../lib/scripts/fde-harden-initramfs.mjs'
import { instrumentProducer } from '../lib/scripts/fde-initramfs-producer.mjs'

const fixture = resolve(process.argv[2] ?? '')
const native = resolve(process.argv[3] ?? '')
assert.ok(
  process.argv[2] && process.argv[3],
  'Provide genuine generated runtime providers and installed initramfs-tools prefix',
)

async function context(t) {
  const temp = await mkdtemp(join(tmpdir(), 'fde-runtime-sources-'))
  t.after(() => rm(temp, { recursive: true, force: true }))
  const root = join(temp, 'source')
  const config = join(temp, 'config')
  const shared = join(root, 'usr/share/initramfs-tools/scripts')
  await mkdir(shared, { recursive: true })
  await cp(join(fixture, 'scripts'), shared, {
    recursive: true,
    verbatimSymlinks: true,
  })
  await mkdir(join(config, 'scripts'), { recursive: true })
  for (const name of ['startos', 'live'])
    await rename(join(shared, name), join(config, 'scripts', name))
  for (const directory of await readdir(shared, { withFileTypes: true }))
    if (directory.isDirectory())
      await rm(join(shared, directory.name, 'ORDER'), { force: true })
  return { temp, root, config, shared }
}

test('original shared and private providers admit without rewriting or executing them', async t => {
  const { root, config, shared } = await context(t)
  const functions = await readFile(join(shared, 'functions'))
  await admitRuntimeSources(root, config)
  assert.deepEqual(await readFile(join(shared, 'functions')), functions)
  await cp(join(shared, 'local'), join(config, 'scripts/local'))
  await admitRuntimeSources(root, config)
})

test('no private runtime override directory needs no replacement tree', async t => {
  const { root, config } = await context(t)
  await rm(join(config, 'scripts'), { recursive: true })
  await admitRuntimeSources(root, config)
})

for (const [name, mutate, pattern] of [
  [
    'changed shared provider',
    async ({ shared }) => writeFile(join(shared, 'init-top/udev'), 'changed'),
    /Unsupported/,
  ],
  [
    'changed private override',
    async ({ config }) => writeFile(join(config, 'scripts/local'), 'changed'),
    /Unsupported/,
  ],
  [
    'missing global functions',
    async ({ shared }) => rm(join(shared, 'functions')),
    /Missing native runtime functions/,
  ],
  [
    'symlink shared provider',
    async ({ shared }) => {
      await rm(join(shared, 'functions'))
      await symlink('/etc/passwd', join(shared, 'functions'))
    },
    /Unsupported generated file type/,
  ],
  [
    'symlink private source directory',
    async ({ config }) => {
      await rm(join(config, 'scripts'), { recursive: true })
      await symlink('/etc', join(config, 'scripts'))
    },
    /Unsupported generated directory/,
  ],
  [
    'unknown credential runtime provider',
    async ({ shared }) => {
      await mkdir(join(shared, 'local-top'))
      await writeFile(
        join(shared, 'local-top/cryptroot'),
        '#!/bin/sh\nexit 0\n',
      )
    },
    /Unsupported/,
  ],
  [
    'generated ORDER used as original executable source',
    async ({ shared }) => {
      await cp(
        join(fixture, 'scripts/init-top/ORDER'),
        join(shared, 'init-top/ORDER'),
      )
    },
    /Generated ordering is not a runtime source/,
  ],
]) {
  test(`${name} rejects before native invocation`, async t => {
    const data = await context(t)
    await mutate(data)
    await assert.rejects(admitRuntimeSources(data.root, data.config), pattern)
  })
}

test('the native entrypoint gates its first source operation on original-provider admission', async t => {
  const { temp, root, config, shared } = await context(t)
  const adapted = instrumentProducer(
    await readFile(join(native, 'usr/sbin/mkinitramfs')),
    await readFile(join(native, 'usr/share/initramfs-tools/hook-functions')),
  )
  const first = adapted.indexOf(': "${FDE_ADMIT_RUNTIME:')
  const end = adapted.indexOf(
    '. /usr/share/initramfs-tools/scripts/functions',
    first,
  )
  assert.ok(first > 0 && end > first)
  const callback = join(temp, 'admit.mjs')
  const module = fileURLToPath(
    new URL('../lib/scripts/fde-harden-initramfs.mjs', import.meta.url),
  )
  await writeFile(
    callback,
    `#!${process.execPath}\nimport { admitRuntimeSources } from ${JSON.stringify(module)}; await admitRuntimeSources(${JSON.stringify(root)}, process.argv[2]);\n`,
    { mode: 0o755 },
  )
  const sentinel = join(temp, 'source-executed')
  const script = join(temp, 'entry')
  await writeFile(
    script,
    `${adapted.slice(first, end)}\nprintf reached > '${sentinel}'\n`,
  )
  const env = {
    PATH: process.env.PATH,
    CONFDIR: config,
    FDE_ADMIT_RUNTIME: callback,
  }
  const result = spawnSync('sh', ['-e', script], { encoding: 'utf8', env })
  assert.equal(result.status, 0, result.stderr)
  await rm(sentinel)
  await writeFile(join(shared, 'functions'), 'changed')
  const rejected = spawnSync('sh', ['-e', script], { encoding: 'utf8', env })
  assert.notEqual(rejected.status, 0)
  assert.match(rejected.stderr, /Unsupported/)
  await assert.rejects(lstat(sentinel), { code: 'ENOENT' })
  const missing = spawnSync('sh', ['-e', script], {
    encoding: 'utf8',
    env: { PATH: process.env.PATH, CONFDIR: config },
  })
  assert.notEqual(missing.status, 0)
  assert.match(missing.stderr, /Missing runtime source admission/)
})
