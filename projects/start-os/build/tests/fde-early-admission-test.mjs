import assert from 'node:assert/strict'
import {
  appendFile,
  chmod,
  cp,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rm,
  writeFile,
} from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'

import { prepareNativeHooks } from '../lib/scripts/fde-initramfs-hooks.mjs'
import { kernelDirectory } from '../lib/scripts/fde-initramfs-paths.mjs'
import { configurationCommands } from '../lib/scripts/fde-initramfs-producer.mjs'
import { runSource } from '../lib/scripts/fde-reviewed-source.mjs'

const source = process.argv[2]
assert.ok(source, 'Genuine hook provider tree required')

async function fixture(t) {
  const directory = await mkdtemp(join(tmpdir(), '.fde-early-test-'))
  t.after(() => rm(directory, { recursive: true, force: true }))
  const hooks = join(directory, 'native')
  await mkdir(hooks)
  for (const name of ['amd64_microcode', 'intel_microcode'])
    await cp(
      join(source, 'usr/share/initramfs-tools/hooks', name),
      join(hooks, name),
    )
  const root = join(directory, 'root')
  await mkdir(root)
  const archive = join(directory, 'early.cpio')
  await writeFile(archive, Buffer.alloc(0), { mode: 0o600 })
  const { version } = await kernelDirectory('/')
  await runSource('/bin/sh', [
    '-c',
    `set -e
export MODULES=most verbose=n version="$2" CONFDIR=/etc/initramfs-tools
export DESTDIR="$1/root" TMPDIR="$1" __TMPEARLYCPIO="$1/early.cpio"
. /usr/share/initramfs-tools/hook-functions
run_scripts "$1/native"
`,
    'native-microcode-fixture',
    directory,
    version,
  ])
  const bytes = await readFile(archive)
  assert.ok(bytes.length > 0, 'Genuine microcode archive required')
  const profile = await prepareNativeHooks(source, join(directory, 'profile'))
  return { directory, archive, bytes, version, profile }
}

async function references() {
  return (await readdir(tmpdir()))
    .filter(
      name =>
        name.startsWith('.fde-early-') && !name.startsWith('.fde-early-test-'),
    )
    .sort()
}

async function admitted(f, configuration = '/etc/initramfs-tools') {
  const before = await references()
  try {
    await f.profile.admitEarlyArchive(f.archive, configuration, f.version)
  } finally {
    assert.deepEqual(await references(), before)
  }
}

test('native configuration initialization restores its source PATH and umask', async () => {
  const { stdout } = await runSource('/bin/sh', [
    '-c',
    `umask 077
PATH=/unsupported
CONFDIR=/etc/initramfs-tools
${configurationCommands(await readFile('/usr/sbin/mkinitramfs'))}
printf '%s\\n' "$(umask)" "$PATH" "$MODULES"
`,
  ])
  assert.equal(stdout, '0022\n/usr/bin:/sbin:/bin\nmost\n')
})

test('native microcode prefix matches admitted source output without changing candidate bytes', async t => {
  const f = await fixture(t)
  const before = await lstat(f.archive)
  await admitted(f)
  assert.deepEqual(await readFile(f.archive), f.bytes)
  const after = await lstat(f.archive)
  assert.equal(after.mode, before.mode)
  assert.equal(after.mtimeMs, before.mtimeMs)
})

test('missing and empty early prefixes reject when native source supplies microcode', async t => {
  const f = await fixture(t)
  await writeFile(f.archive, Buffer.alloc(0))
  await assert.rejects(admitted(f))
  await rm(f.archive)
  await assert.rejects(admitted(f))
})

test('changed and truncated native microcode prefixes reject with reference cleanup', async t => {
  const f = await fixture(t)
  const changed = Buffer.from(f.bytes)
  changed[Math.floor(changed.length / 2)] ^= 1
  await writeFile(f.archive, changed)
  await assert.rejects(admitted(f))
  await writeFile(f.archive, f.bytes.subarray(0, f.bytes.length - 1))
  await assert.rejects(admitted(f))
})

test('an appended native archive cannot add an executable boot route to the early prefix', async t => {
  const f = await fixture(t)
  const extra = join(f.directory, 'extra')
  await mkdir(extra)
  await writeFile(join(extra, 'init'), '#!/bin/sh\nexit 0\n', { mode: 0o700 })
  await writeFile(join(f.directory, 'names'), 'init\n')
  await runSource('/bin/sh', [
    '-c',
    'set -e; cd "$1/extra"; cpio --quiet -o -H newc < "$1/names" > "$1/extra.cpio"',
    'native-extra-archive',
    f.directory,
  ])
  const payload = await readFile(join(f.directory, 'extra.cpio'))
  assert.ok(payload.length > 0)
  await appendFile(f.archive, payload)
  await assert.rejects(admitted(f))
})

test('early-prefix permissions and nonregular paths reject without changing admitted bytes', async t => {
  const f = await fixture(t)
  await chmod(f.archive, 0o644)
  await assert.rejects(admitted(f))
  await rm(f.archive)
  await mkdir(f.archive)
  await assert.rejects(admitted(f))
})

test('source profiles without optional microcode hooks admit empty prefixes and reject injected data', async t => {
  const f = await fixture(t)
  const sourceRoot = join(f.directory, 'no-microcode')
  const hooks = join(sourceRoot, 'usr/share/initramfs-tools/hooks')
  await cp(join(source, 'usr/share/initramfs-tools/hooks'), hooks, {
    recursive: true,
  })
  for (const name of ['amd64_microcode', 'intel_microcode'])
    await rm(join(hooks, name))
  f.profile = await prepareNativeHooks(
    sourceRoot,
    join(f.directory, 'empty-profile'),
  )
  await assert.rejects(admitted(f))
  await writeFile(f.archive, Buffer.alloc(0))
  await admitted(f)
})

test('native early-hook failures reject and remove their reference workspace', async t => {
  const f = await fixture(t)
  f.version = '1'
  await assert.rejects(admitted(f), /unsupported kernel version/)
})

test('early admission rechecks original public configuration before native reference execution', async t => {
  const f = await fixture(t)
  const configuration = join(f.directory, 'configuration')
  await cp('/etc/initramfs-tools', configuration, { recursive: true })
  await appendFile(join(configuration, 'initramfs.conf'), '\nMODULES=list\n')
  await assert.rejects(
    admitted(f, configuration),
    /Unsupported generated context/,
  )
})
