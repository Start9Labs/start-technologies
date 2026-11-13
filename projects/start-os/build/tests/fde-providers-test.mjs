import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import {
  chmod,
  copyFile,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  readlink,
  realpath,
  rm,
  symlink,
  writeFile,
} from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, relative } from 'node:path'
import test from 'node:test'

import { prepareNativeHooks } from '../lib/scripts/fde-initramfs-hooks.mjs'
import { requireNativeHookFunctions } from '../lib/scripts/fde-initramfs-producer.mjs'
import { runSource } from '../lib/scripts/fde-reviewed-source.mjs'

const source = process.argv[2]
assert.ok(source, 'Provide admitted genuine hook sources')
requireNativeHookFunctions(
  await readFile('/usr/share/initramfs-tools/hook-functions'),
)

function shell(script, env = {}) {
  return spawnSync('/bin/bash', ['-se'], {
    input: script,
    encoding: 'utf8',
    env: { PATH: '/usr/sbin:/usr/bin:/sbin:/bin', LC_ALL: 'C', ...env },
  })
}

async function context(t, BUSYBOX = 'y') {
  const directory = await mkdtemp('/var/tmp/fde-providers-')
  t.after(() => rm(directory, { recursive: true, force: true }))
  const hooks = join(directory, 'hooks')
  const admitted = await prepareNativeHooks(source, hooks)
  const root = join(directory, 'root')
  for (const path of [
    'usr/bin',
    'usr/sbin',
    'usr/lib',
    'usr/lib64',
    'etc',
    'var/tmp',
  ])
    await mkdir(join(root, path), { recursive: true })
  for (const path of ['bin', 'sbin', 'lib', 'lib64'])
    await symlink(`usr/${path}`, join(root, path))
  const result = shell(
    `. /usr/share/initramfs-tools/hook-functions
copy_exec /usr/bin/cat
copy_file binary /usr/lib/klibc/bin/fstype
sh "$FDE_HOOKS/zz-busybox"
sh "$FDE_HOOKS/klibc-utils"
`,
    {
      DESTDIR: root,
      BUSYBOX,
      BUSYBOXDIR: '/bin',
      verbose: 'n',
      FDE_HOOKS: hooks,
    },
  )
  assert.equal(result.status, 0, result.stderr)
  return { directory, hooks, admitted, root }
}

for (const BUSYBOX of ['y', 'n']) {
  test(`native BUSYBOX=${BUSYBOX} providers admit before copied commands execute`, async t => {
    const { root, admitted } = await context(t, BUSYBOX)
    await admitted.admitProviders(root)
    assert.equal(
      (await lstat(join(root, 'usr/bin/cat'))).isSymbolicLink(),
      false,
    )
    assert.equal(
      await realpath(join(root, 'usr/bin/fstype')),
      join(root, 'usr/lib/klibc/bin/fstype'),
    )
    assert.ok(!(await readlink(join(root, 'usr/bin/fstype'))).startsWith('/'))
    for (const excluded of ['gzip', 'kinit', 'zcat'])
      await assert.rejects(lstat(join(root, 'usr/lib/klibc/bin', excluded)), {
        code: 'ENOENT',
      })
    await writeFile(join(root, 'blank'), '')
    const sourceType = shell('/usr/lib/klibc/bin/fstype "$BLANK"', {
      BLANK: join(root, 'blank'),
    })
    const copiedType = shell('chroot "$ROOT" /usr/bin/fstype /blank', {
      ROOT: root,
    })
    assert.equal(sourceType.status, 2, sourceType.stderr)
    assert.equal(copiedType.status, sourceType.status, copiedType.stderr)
    assert.equal(copiedType.stdout, sourceType.stdout)
    assert.match(copiedType.stderr, /Not a typewriter/)
    await mkdir(join(root, 'to-remove'))
    const nuke = shell('chroot "$ROOT" /usr/bin/nuke /to-remove', {
      ROOT: root,
    })
    assert.equal(nuke.status, 0, nuke.stderr)
    await assert.rejects(lstat(join(root, 'to-remove')), { code: 'ENOENT' })
    if (BUSYBOX === 'y') {
      const actual = shell(
        'chroot "$ROOT" /usr/bin/sh -c "printf busybox-runtime-ok"',
        { ROOT: root },
      )
      assert.equal(actual.status, 0, actual.stderr)
      assert.equal(actual.stdout, 'busybox-runtime-ok')
    }
  })
}

for (const [name, mutate, pattern] of [
  [
    'missing native shared dependency',
    async root => {
      const result = shell('find "$ROOT/usr" -type f -name libc.so.6', {
        ROOT: root,
      })
      assert.equal(result.status, 0, result.stderr)
      const paths = result.stdout.trim().split('\n')
      assert.equal(paths.length, 1)
      await rm(paths[0])
    },
    /ENOENT/,
  ],
  [
    'missing ELF interpreter directory aliases',
    async root => {
      await rm(join(root, 'lib'))
      await rm(join(root, 'lib64'))
    },
    /ENOENT/,
  ],
  [
    'missing klibc interpreter',
    async root => {
      const names = (await readdir(join(root, 'usr/lib'))).filter(
        name => name.startsWith('klibc-') && name.endsWith('.so'),
      )
      assert.equal(names.length, 1)
      await rm(join(root, 'usr/lib', names[0]))
    },
    /ENOENT/,
  ],
  [
    'another genuine executable at a trusted command path',
    root => copyFile('/usr/bin/true', join(root, 'usr/bin/cat')),
    /cmp/,
  ],
  [
    'relocated genuine ELF',
    root => copyFile('/usr/bin/busybox', join(root, 'usr/bin/unadmitted')),
    /ENOENT/,
  ],
  [
    'provider permission change',
    root => chmod(join(root, 'usr/bin/cat'), 0o777),
    /permissions differ/,
  ],
  [
    'unknown command alias',
    root => symlink('busybox', join(root, 'usr/bin/unadmitted')),
    /Unsupported copied provider alias/,
  ],
  [
    'existing alias redirected to another genuine command',
    async root => {
      await rm(join(root, 'usr/bin/ls'))
      await symlink('cat', join(root, 'usr/bin/ls'))
    },
    /Unsupported copied provider alias/,
  ],
  [
    'absolute provider link outside the candidate',
    async root => {
      await rm(join(root, 'usr/bin/ls'))
      await symlink('/usr/bin/ls', join(root, 'usr/bin/ls'))
    },
    /must be relative/,
  ],
  [
    'absolute provider link into the build directory',
    async root => {
      await rm(join(root, 'usr/bin/ls'))
      await symlink(join(root, 'usr/bin/busybox'), join(root, 'usr/bin/ls'))
    },
    /must be relative/,
  ],
  [
    'relative link escaping the candidate',
    async root => {
      await rm(join(root, 'usr/bin/ls'))
      await symlink(
        relative(join(root, 'usr/bin'), '/usr/bin/ls'),
        join(root, 'usr/bin/ls'),
      )
    },
    /escapes public tree/,
  ],
  [
    'relative link leaving and re-entering the build directory',
    async root => {
      await rm(join(root, 'usr/bin/ls'))
      await symlink(
        `${relative(join(root, 'usr/bin'), '/')}/${root.slice(1)}/usr/bin/busybox`,
        join(root, 'usr/bin/ls'),
      )
    },
    /not canonical relative/,
  ],
  [
    'klibc command redirected to another eligible provider',
    async root => {
      await rm(join(root, 'usr/bin/fstype'))
      await symlink('../lib/klibc/bin/minips', join(root, 'usr/bin/fstype'))
    },
    /Unsupported copied provider alias/,
  ],
  [
    'FIFO in the provider tree',
    root => {
      const result = shell('mkfifo "$FILE"', {
        FILE: join(root, 'usr/bin/unadmitted'),
      })
      assert.equal(result.status, 0, result.stderr)
    },
    /Unsupported provider file type/,
  ],
  [
    'root command-directory alias redirected',
    async root => {
      await rm(join(root, 'bin'))
      await symlink('usr/sbin', join(root, 'bin'))
    },
    /Unsupported copied provider alias/,
  ],
]) {
  test(`${name} rejects passively`, async t => {
    const { root, admitted } = await context(t)
    await mutate(root)
    const references = async () =>
      (await readdir(tmpdir()))
        .filter(name => name.startsWith('.fde-dependencies-'))
        .sort()
    const before = await references()
    await assert.rejects(admitted.admitProviders(root), pattern)
    assert.deepEqual(await references(), before)
  })
}

test('source commands exclude inherited loader and diagnostic overrides', async () => {
  const previous = process.env.TSS2_LOG
  process.env.TSS2_LOG = 'all+DEBUG'
  try {
    const { stdout } = await runSource('/usr/bin/env', [])
    assert.deepEqual(stdout.trim().split('\n').sort(), [
      'LC_ALL=C',
      'PATH=/usr/sbin:/usr/bin:/sbin:/bin',
    ])
  } finally {
    if (previous === undefined) delete process.env.TSS2_LOG
    else process.env.TSS2_LOG = previous
  }
})

test('provider metadata is read-only and retains native klibc exclusions', async t => {
  const { directory, admitted } = await context(t)
  const before = await readdir(directory)
  const aliases = new Map(await admitted.providerAliases())
  assert.equal(aliases.get('bin/linuxrc'), '/usr/bin/busybox')
  assert.equal(aliases.get('/bin/fstype'), '/usr/lib/klibc/bin/fstype')
  for (const [, provider] of aliases)
    assert.ok(
      !['gzip', 'kinit', 'zcat'].some(
        name => provider === `/usr/lib/klibc/bin/${name}`,
      ),
    )
  assert.deepEqual(await readdir(directory), before)
})

test('native klibc copy failure rejects the private hook', async t => {
  const { directory, hooks } = await context(t)
  const root = join(directory, 'failed')
  await mkdir(join(root, 'usr/lib'), { recursive: true })
  await mkdir(join(root, 'usr/bin'), { recursive: true })
  await symlink('usr/bin', join(root, 'bin'))
  await symlink('usr/lib', join(root, 'lib'))
  await writeFile(join(root, 'usr/lib/klibc'), 'occupied')
  const result = shell('sh "$FDE_HOOKS/klibc-utils"', {
    DESTDIR: root,
    BUSYBOX: 'n',
    BUSYBOXDIR: '/bin',
    verbose: 'n',
    FDE_HOOKS: hooks,
  })
  assert.notEqual(result.status, 0)
  assert.match(result.stderr, /Not a directory|not a directory/)
})
