import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import {
  chmod,
  cp,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rm,
  symlink,
  writeFile,
} from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import test from 'node:test'

import { prepareNativeHooks } from '../lib/scripts/fde-initramfs-hooks.mjs'
import { instrumentProducer } from '../lib/scripts/fde-initramfs-producer.mjs'

const source = resolve(process.argv[2] ?? '')
const prefix = resolve(process.argv[3] ?? '')
assert.ok(
  process.argv[2] && process.argv[3],
  'Provide genuine native hook/config sources and an installed initramfs-tools prefix',
)

async function context(t) {
  const temp = await mkdtemp(join(tmpdir(), 'fde-source-hooks-'))
  t.after(() => rm(temp, { recursive: true, force: true }))
  const root = join(temp, 'source')
  await cp(source, root, { recursive: true, verbatimSymlinks: true })
  return { temp, root, target: join(temp, 'admitted') }
}

const hookPath = (root, name) =>
  join(root, 'usr/share/initramfs-tools/hooks', name)

test('reviewed native hooks are copied privately without changing their installed sources', async t => {
  const { root, target } = await context(t)
  await prepareNativeHooks(root, target)
  const comparison = spawnSync(
    'diff',
    ['-r', '--no-dereference', source, root],
    { encoding: 'utf8' },
  )
  assert.equal(comparison.status, 0, comparison.stdout + comparison.stderr)
  assert.equal((await lstat(target)).mode & 0o777, 0o700)
  const names = await readdir(target)
  assert.equal(names.length, 10)
  for (const name of names) {
    const syntax = spawnSync('sh', ['-n', join(target, name)], {
      encoding: 'utf8',
    })
    assert.equal(syntax.status, 0, syntax.stderr)
    const mode = (await lstat(join(target, name))).mode
    assert.equal(mode & 0o700, 0o700)
    assert.equal(mode & 0o022, 0)
  }
  assert.ok(!names.includes('cryptroot'))
  assert.ok(!names.includes('resume'))
  assert.ok(!names.includes('keymap'))
  assert.ok(!names.includes('fuse'))
})

test('provider layout adaptations preserve native prerequisite dispatch', async t => {
  const { root, target } = await context(t)
  await prepareNativeHooks(root, target)
  for (const BUSYBOX of ['y', 'n']) {
    const options = {
      encoding: 'utf8',
      env: { PATH: process.env.PATH, BUSYBOX, BUSYBOXDIR: '/bin' },
    }
    for (const name of ['zz-busybox', 'klibc-utils']) {
      const original = spawnSync(
        'sh',
        [hookPath(root, name), 'prereqs'],
        options,
      )
      const adapted = spawnSync('sh', [join(target, name), 'prereqs'], options)
      assert.equal(original.status, 0, original.stderr)
      assert.equal(adapted.status, 0, adapted.stderr)
      assert.equal(adapted.stdout, original.stdout)
    }
  }
})

test('absent microcode packages and configuration preserve native defaults', async t => {
  const { root, target } = await context(t)
  for (const name of ['intel_microcode', 'amd64_microcode'])
    await rm(hookPath(root, name))
  await rm(join(root, 'etc/default'), { recursive: true })
  await prepareNativeHooks(root, target)
  assert.equal((await readdir(target)).length, 8)
})

test('absent configuration retains the native microcode defaults', async t => {
  const { root, target } = await context(t)
  await rm(join(root, 'etc/default'), { recursive: true })
  await prepareNativeHooks(root, target)
  assert.equal((await readdir(target)).length, 10)
})

test('the package timestamp remains metadata rather than a second producer implementation', async t => {
  const { root, target } = await context(t)
  const path = hookPath(root, 'amd64_microcode')
  const input = await readFile(path, 'utf8')
  await writeFile(
    path,
    input.replace(/^CHANGELOG_TS=[0-9]+$/m, 'CHANGELOG_TS=123'),
  )
  await prepareNativeHooks(root, target)
  assert.ok(
    (await readFile(join(target, 'amd64_microcode'), 'utf8')).includes(
      'CHANGELOG_TS=123\n',
    ),
  )
})

for (const [name, mutate, pattern] of [
  [
    'changed hook',
    root => writeFile(hookPath(root, 'kmod'), 'changed'),
    /Unsupported/,
  ],
  [
    'unknown hook',
    root => writeFile(hookPath(root, 'unadmitted'), '#!/bin/sh\nexit 0\n'),
    /Unsupported native hook/,
  ],
  [
    'missing required hook',
    root => rm(hookPath(root, 'kmod')),
    /Missing native hook/,
  ],
  [
    'symlink hook',
    async root => {
      await rm(hookPath(root, 'kmod'))
      await symlink('/etc/passwd', hookPath(root, 'kmod'))
    },
    /Unsupported native hook file/,
  ],
  [
    'disabled hook',
    root => chmod(hookPath(root, 'kmod'), 0o644),
    /Unsupported native hook file/,
  ],
  [
    'changed microcode configuration',
    root =>
      writeFile(
        join(root, 'etc/default/intel-microcode'),
        'IUCODE_TOOL_EXTRA_OPTIONS=unexpected\n',
      ),
    /Unsupported/,
  ],
  [
    'symlink microcode configuration',
    async root => {
      const path = join(root, 'etc/default/amd64-microcode')
      await rm(path)
      await symlink('/etc/passwd', path)
    },
    /Unsupported microcode configuration/,
  ],
  [
    'executable timestamp',
    async root => {
      const path = hookPath(root, 'amd64_microcode')
      await writeFile(
        path,
        (await readFile(path, 'utf8')).replace(
          /^CHANGELOG_TS=.*$/m,
          'CHANGELOG_TS=$(false)',
        ),
      )
    },
    /Unsupported AMD microcode timestamp/,
  ],
]) {
  test(`${name} rejects before private hook publication`, async t => {
    const { root, target } = await context(t)
    await mutate(root)
    await assert.rejects(prepareNativeHooks(root, target), pattern)
    await assert.rejects(lstat(target), { code: 'ENOENT' })
  })
}

test('native source directories cannot become private output or be overwritten by reuse', async t => {
  const { root, target } = await context(t)
  const inside = hookPath(root, 'private')
  await assert.rejects(prepareNativeHooks(root, inside), /inside source hooks/)
  await assert.rejects(lstat(inside), { code: 'ENOENT' })
  await prepareNativeHooks(root, target)
  await assert.rejects(prepareNativeHooks(root, target), { code: 'EEXIST' })
})

test('the native dependency runner uses only the private source directory', async t => {
  const { temp } = await context(t)
  const native = await readFile(join(prefix, 'usr/sbin/mkinitramfs'))
  const functions = await readFile(
    join(prefix, 'usr/share/initramfs-tools/hook-functions'),
  )
  const adapted = instrumentProducer(native, functions)
  assert.ok(!adapted.includes('run_scripts /usr/share/initramfs-tools/hooks'))
  const first = adapted.indexOf(': "${FDE_NATIVE_HOOKS:')
  const end = adapted.indexOf('\nrun_scripts "${CONFDIR}"/hooks', first)
  assert.ok(first > 0 && end > first)
  const directory = join(temp, 'runner-input')
  await mkdir(directory)
  for (const [name, prerequisite, message] of [
    ['zzz-first', '', 'first'],
    ['aaa-last', 'zzz-first', 'last'],
  ])
    await writeFile(
      join(directory, name),
      `#!/bin/sh\nif [ "\${1-}" = prereqs ]; then echo '${prerequisite}'; exit 0; fi\nprintf '${message}\\n'\n`,
      { mode: 0o755 },
    )
  const script = join(temp, 'dispatch')
  await writeFile(
    script,
    `. '${join(prefix, 'usr/share/initramfs-tools/hook-functions')}'\nversion=7.2.12\nverbose=n\n${adapted.slice(first, end)}\n`,
  )
  const result = spawnSync('sh', ['-e', script], {
    encoding: 'utf8',
    env: { PATH: process.env.PATH, FDE_NATIVE_HOOKS: directory },
  })
  assert.equal(result.status, 0, result.stderr)
  assert.equal(result.stdout, 'first\nlast\n')
  const missing = spawnSync('sh', ['-e', script], {
    encoding: 'utf8',
    env: { PATH: process.env.PATH },
  })
  assert.notEqual(missing.status, 0)
  assert.match(missing.stderr, /Missing admitted private native hooks/)
})
