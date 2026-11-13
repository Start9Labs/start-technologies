import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import {
  chmod,
  copyFile,
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
import { join } from 'node:path'
import test from 'node:test'

import { admitKernelModules } from '../lib/scripts/fde-initramfs-modules.mjs'
import { kernelDirectory } from '../lib/scripts/fde-initramfs-paths.mjs'
import { runSource } from '../lib/scripts/fde-reviewed-source.mjs'

const kernel = await kernelDirectory('/')
const core = `${kernel.path}/kernel/drivers/char/tpm/st33zp24/tpm_st33zp24.ko.xz`
const dependent = `${kernel.path}/kernel/drivers/char/tpm/st33zp24/tpm_st33zp24_i2c.ko.xz`

async function context(t) {
  const root = await mkdtemp('/var/tmp/fde-module-candidate-')
  t.after(() => rm(root, { recursive: true, force: true }))
  await mkdir(join(root, kernel.path), { recursive: true })
  await symlink('usr/lib', join(root, 'lib'))
  const result = spawnSync(
    '/bin/sh',
    [
      '-e',
      '-c',
      `
. /usr/share/initramfs-tools/hook-functions
copy_exec /usr/sbin/modprobe
for library in /usr/lib/*/libkmod.so.2; do copy_exec "$library"; done
: > "$__MODULES_TO_ADD"
manual_add_modules '=drivers/char/tpm'
apply_add_modules
for metadata in modules.builtin modules.builtin.bin modules.builtin.modinfo modules.order; do
cp -p "/usr/lib/modules/$version/$metadata" "$DESTDIR/usr/lib/modules/$version/$metadata"
done
/usr/sbin/depmod -a -b "$DESTDIR" "$version"
`,
    ],
    {
      encoding: 'utf8',
      env: {
        PATH: '/usr/sbin:/usr/bin:/sbin:/bin',
        LC_ALL: 'C',
        verbose: 'n',
        version: kernel.version,
        DESTDIR: root,
        __MODULES_TO_ADD: join(root, 'module-request'),
      },
    },
  )
  assert.equal(result.status, 0, result.stderr)
  return root
}

async function references() {
  return (await readdir(tmpdir()))
    .filter(name => name.startsWith('.fde-modules-'))
    .sort()
}

test('genuine native TPM modules and subset metadata admit without insertion', async t => {
  const root = await context(t)
  const before = await references()
  await admitKernelModules(root)
  assert.deepEqual(await references(), before)
  const dependencies = await runSource('/usr/sbin/modprobe', [
    '--dirname',
    root,
    '--set-version',
    kernel.version,
    '--show-depends',
    'tpm_st33zp24_i2c',
  ])
  assert.match(dependencies.stdout, /tpm_st33zp24\.ko\.xz/)
  assert.match(dependencies.stdout, /tpm_st33zp24_i2c\.ko\.xz/)
})

test('effective soft dependencies already present retain native resolution', async t => {
  const root = await context(t)
  await mkdir(join(root, 'etc/modprobe.d'), { recursive: true })
  await writeFile(
    join(root, 'etc/modprobe.d/extra.conf'),
    'softdep tpm_st33zp24_i2c pre: tpm_st33zp24\n',
  )
  await admitKernelModules(root)
})

for (const [name, dependency] of [
  ['present weak dependency', 'tpm_st33zp24'],
  [
    'unavailable optional weak dependency',
    'startos_fde_unavailable_fixture_module',
  ],
]) {
  test(`${name} retains native lookup semantics`, async t => {
    const root = await context(t)
    await mkdir(join(root, 'etc/modprobe.d'), { recursive: true })
    await writeFile(
      join(root, 'etc/modprobe.d/weak.conf'),
      `weakdep tpm_st33zp24_i2c ${dependency}\n`,
    )
    await admitKernelModules(root)
  })
}

test('weak dependency cycles remain native one-hop queries', async t => {
  const root = await context(t)
  await mkdir(join(root, 'etc/modprobe.d'), { recursive: true })
  await writeFile(
    join(root, 'etc/modprobe.d/weak.conf'),
    'weakdep tpm_st33zp24_i2c tpm_st33zp24\nweakdep tpm_st33zp24 tpm_st33zp24_i2c\n',
  )
  await admitKernelModules(root)
})

test('weak dependency configuration keeps native directory precedence', async t => {
  const root = await context(t)
  for (const directory of ['usr/lib/modprobe.d', 'etc/modprobe.d'])
    await mkdir(join(root, directory), { recursive: true })
  await writeFile(
    join(root, 'usr/lib/modprobe.d/weak.conf'),
    'weakdep tpm_st33zp24_i2c dummy\n',
  )
  await writeFile(
    join(root, 'etc/modprobe.d/weak.conf'),
    'weakdep tpm_st33zp24_i2c tpm_st33zp24\n',
  )
  await admitKernelModules(root)
})

for (const [name, mutate, pattern] of [
  [
    'transitive available weak dependency omitted from candidate',
    async root => {
      await mkdir(join(root, 'etc/modprobe.d'), { recursive: true })
      await writeFile(
        join(root, 'etc/modprobe.d/weak.conf'),
        'weakdep tpm_st33zp24_i2c tpm_st33zp24\nweakdep tpm_st33zp24 dummy\n',
      )
    },
    /Missing native module dependency: .*dummy\.ko/,
  ],
  [
    'available weak dependency omitted from candidate',
    async root => {
      await mkdir(join(root, 'etc/modprobe.d'), { recursive: true })
      await writeFile(
        join(root, 'etc/modprobe.d/weak.conf'),
        'weakdep tpm_st33zp24_i2c dummy\n',
      )
    },
    /Missing native module dependency: .*dummy\.ko/,
  ],
  [
    'effective configuration adds a missing soft dependency',
    async root => {
      await mkdir(join(root, 'etc/modprobe.d'), { recursive: true })
      await writeFile(
        join(root, 'etc/modprobe.d/extra.conf'),
        'softdep tpm_st33zp24_i2c pre: dummy\n',
      )
    },
    /Missing native module dependency: .*dummy\.ko/,
  ],
  [
    'effective configuration invokes an external command',
    async root => {
      await mkdir(join(root, 'etc/modprobe.d'), { recursive: true })
      await writeFile(
        join(root, 'etc/modprobe.d/extra.conf'),
        'install tpm_st33zp24_i2c /bin/false\n',
      )
    },
    /Pre-verification module configuration invokes external commands/,
  ],
  [
    'module byte substitution',
    root => copyFile(join('/', dependent), join(root, core)),
    /Command failed/,
  ],
  [
    'module mode change',
    root => chmod(join(root, core), 0o777),
    /permissions differ/,
  ],
  [
    'text metadata mutation',
    async root => {
      const path = join(root, kernel.path, 'modules.dep')
      await writeFile(path, `${await readFile(path)}\n`)
    },
    /Command failed/,
  ],
  [
    'binary metadata substitution',
    root =>
      copyFile(
        join('/', kernel.path, 'modules.dep.bin'),
        join(root, kernel.path, 'modules.dep.bin'),
      ),
    /Command failed/,
  ],
  [
    'missing generated metadata',
    root => rm(join(root, kernel.path, 'modules.alias.bin')),
    /Missing generated module metadata/,
  ],
  [
    'raw builtin metadata mutation',
    root => writeFile(join(root, kernel.path, 'modules.builtin'), ''),
    /Command failed/,
  ],
  [
    'module symlink',
    async root => {
      await rm(join(root, core))
      await symlink(join('/', core), join(root, core))
    },
    /Unsupported generated file type/,
  ],
  [
    'missing raw source input',
    root => rm(join(root, kernel.path, 'modules.order')),
    /Missing source module metadata/,
  ],
  [
    'FIFO in module tree',
    root =>
      runSource('/usr/bin/mkfifo', [join(root, kernel.path, 'unexpected')]),
    /Unsupported generated file type/,
  ],
  [
    'second kernel version',
    root => mkdir(join(root, 'usr/lib/modules/other')),
    /Expected one generated kernel/,
  ],
  [
    'wrong source kernel version',
    root =>
      rename(join(root, kernel.path), join(root, 'usr/lib/modules/other')),
    /version differs from source/,
  ],
  [
    'missing dependency despite regenerated indexes',
    async root => {
      await rm(join(root, core))
      await runSource('/usr/sbin/depmod', [
        '--all',
        `--basedir=${root}`,
        kernel.version,
      ])
    },
    /Missing native module dependency/,
  ],
]) {
  test(`${name} rejects`, async t => {
    const root = await context(t)
    await mutate(root)
    const before = await references()
    await assert.rejects(admitKernelModules(root), pattern)
    assert.deepEqual(await references(), before)
  })
}
