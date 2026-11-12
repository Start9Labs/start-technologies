import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { spawn, spawnSync } from 'node:child_process'
import {
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
import { once } from 'node:events'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

import { harden } from '../lib/scripts/fde-harden-initramfs.mjs'

const fixture = resolve(process.argv[2] ?? '')
assert.ok(
  process.argv[2] && process.argv[3],
  'Provide reviewed generated scripts/configuration and packaged udev rules in one fixture tree, plus BusyBox',
)
const shells = [
  { name: 'dash', program: 'sh', args: [] },
  { name: 'busybox', program: resolve(process.argv[3]), args: ['sh'] },
]

async function snapshot(root) {
  const entries = []
  for (const name of (await readdir(root)).sort()) {
    const path = join(root, name)
    const info = await lstat(path)
    entries.push([
      name,
      info.mode,
      info.isFile()
        ? createHash('sha256')
            .update(await readFile(path))
            .digest('hex')
        : info.isDirectory()
          ? await snapshot(path)
          : 'nonregular',
    ])
  }
  return entries
}

async function copyFixture(t) {
  const temp = await mkdtemp(join(tmpdir(), 'fde-initramfs-'))
  t.after(() => rm(temp, { recursive: true, force: true }))
  const tree = join(temp, 'tree')
  await cp(fixture, tree, { recursive: true })
  return { temp, tree }
}

async function assertStops(temp, prefix, operation, shell, rebootStatus = 0) {
  const log = join(temp, 'failure.log')
  const script = `${prefix}\nreboot() { printf 'reboot\\n' >> '${log}'; return ${rebootStatus}; }\nsleep() { printf 'sleep\\n' >> '${log}'; printf 'waiting\\n'; /bin/sleep 60; }\n${operation}\nprintf 'fell-through\\n' >> '${log}'\n`
  const child = spawn(shell.program, [...shell.args, '-ec', script], {
    detached: true,
    stdio: ['ignore', 'pipe', 'ignore'],
  })
  const exited = new Promise((resolve, reject) => {
    child.once('exit', resolve)
    child.once('error', reject)
  })
  try {
    const [ready] = await Promise.race([
      once(child.stdout, 'data', { signal: AbortSignal.timeout(3000) }),
      exited.then(() => {
        throw Error('Failure handler exited instead of stopping boot')
      }),
    ])
    assert.equal(ready.toString(), 'waiting\n')
  } finally {
    try {
      process.kill(-child.pid, 'SIGKILL')
    } catch (error) {
      if (error.code !== 'ESRCH') throw error
    }
    await exited
  }
  assert.equal(await readFile(log, 'utf8'), 'reboot\nsleep\n')
}

test('reviewed control plane hardens without mutating its ordinary source', async t => {
  const before = await snapshot(fixture)
  const { tree } = await copyFixture(t)
  await harden(tree)
  assert.deepEqual(await snapshot(fixture), before)
  const init = await readFile(join(tree, 'init'), 'utf8')
  const functions = await readFile(join(tree, 'scripts/functions'), 'utf8')
  assert.ok(init.includes("trap 'fde_fail' EXIT"))
  assert.ok(init.includes('BOOT=startos-fde\nIP=none\nnoresume=y'))
  assert.ok(!init.includes('for inittest'))
  assert.ok(!init.includes('read_fstab_entry /usr'))
  assert.ok(!init.includes('for conf in conf/conf.d'))
  assert.ok(!functions.includes('sh -i'))
  for (const path of [
    'scripts/live',
    'scripts/startos',
    'scripts/init-top/keymap',
    'scripts/local-premount/resume',
    'conf/conf.d/cryptsetup',
  ]) {
    await assert.rejects(lstat(join(tree, path)), { code: 'ENOENT' })
  }
  for (const path of [
    'scripts/init-top/ORDER',
    'scripts/init-bottom/ORDER',
    'scripts/local-premount/ORDER',
  ]) {
    const order = await readFile(join(tree, path), 'utf8')
    assert.ok(!order.includes('param.conf'))
    assert.ok(!order.includes('keymap'))
    assert.ok(!order.includes('resume'))
  }
  const rules = join(tree, 'usr/lib/udev/rules.d')
  const defaults = await readFile(join(rules, '50-udev-default.rules'), 'utf8')
  const names = await readFile(
    join(rules, '73-special-net-names.rules'),
    'utf8',
  )
  assert.ok(!defaults.includes('REMOVE_CMD'))
  assert.ok(defaults.includes('ACTION=="remove", GOTO="default_end"'))
  assert.ok(!names.includes('/bin/sh'))
  assert.ok(!names.includes('ibmveth'))
  assert.ok(names.includes('NAME="idrac"'))
  assert.ok(names.includes('NAME="ibmimm"'))
  for (const name of [
    '50-firmware.rules',
    '60-block.rules',
    '60-persistent-storage.rules',
    '75-net-description.rules',
    '80-net-setup-link.rules',
    '80-drivers.rules',
  ])
    assert.deepEqual(
      await readFile(join(rules, name)),
      await readFile(join(fixture, 'usr/lib/udev/rules.d', name)),
    )
  const verification = spawnSync(
    'udevadm',
    ['verify', '--resolve-names=never', '--no-style', `--root=${tree}`],
    { encoding: 'utf8' },
  )
  assert.equal(verification.status, 0, verification.stderr)
  for (const shell of shells)
    assert.equal(
      spawnSync(shell.program, [...shell.args, '-n', join(tree, 'init')])
        .status,
      0,
    )
})

test('the executable entrypoint hardens when invoked through a symlink', async t => {
  const { temp, tree } = await copyFixture(t)
  const entrypoint = join(temp, 'harden.mjs')
  await symlink(
    fileURLToPath(
      new URL('../lib/scripts/fde-harden-initramfs.mjs', import.meta.url),
    ),
    entrypoint,
  )
  const result = spawnSync(process.execPath, [entrypoint, tree], {
    encoding: 'utf8',
  })
  assert.equal(result.status, 0, result.stderr)
  assert.ok(
    (await readFile(join(tree, 'init'), 'utf8')).includes(
      "trap 'fde_fail' EXIT",
    ),
  )
})

test('canonical usr merge and absent optional package rules are supported', async t => {
  const { tree } = await copyFixture(t)
  await symlink('usr/lib', join(tree, 'lib'))
  for (const directory of [
    'etc/udev/rules.d',
    'run/udev/rules.d',
    'usr/local/lib/udev/rules.d',
  ])
    await mkdir(join(tree, directory), { recursive: true })
  for (const name of [
    '55-dm.rules',
    '60-persistent-storage-dm.rules',
    '64-btrfs.rules',
    '64-btrfs-dm.rules',
    '64-btrfs-zoned.rules',
  ])
    await rm(join(tree, 'usr/lib/udev/rules.d', name), { force: true })
  await harden(tree)
  assert.ok(
    (
      await readFile(
        join(tree, 'lib/udev/rules.d/50-udev-default.rules'),
        'utf8',
      )
    ).includes('default_end'),
  )
})

for (const [name, mutate] of [
  [
    'changed init',
    tree => writeFile(join(tree, 'init'), '#!/bin/sh\nexec sh -i\n'),
  ],
  [
    'changed functions',
    tree => writeFile(join(tree, 'scripts/functions'), 'panic() { sh -i; }\n'),
  ],
  [
    'new provider',
    tree =>
      writeFile(join(tree, 'scripts/init-top/dropbear'), '#!/bin/sh\nsh -i\n'),
  ],
  [
    'new executable configuration',
    tree => writeFile(join(tree, 'conf/conf.d/recovery'), 'sh -i\n'),
  ],
  [
    'changed ordering',
    tree => writeFile(join(tree, 'scripts/init-top/ORDER'), '/bin/sh -i\n'),
  ],
  [
    'symlink provider',
    async tree => {
      await rm(join(tree, 'scripts/init-top/udev'))
      await symlink('/bin/sh', join(tree, 'scripts/init-top/udev'))
    },
  ],
  ['missing provider', tree => rm(join(tree, 'scripts/init-bottom/udev'))],
  [
    'executable architecture',
    tree => writeFile(join(tree, 'conf/arch.conf'), 'DPKG_ARCH=amd64; sh -i\n'),
  ],
  [
    'changed udev rule',
    tree =>
      writeFile(
        join(tree, 'usr/lib/udev/rules.d/50-udev-default.rules'),
        'RUN+="/bin/sh -i"\n',
      ),
  ],
  [
    'unreviewed udev rule',
    tree =>
      writeFile(
        join(tree, 'usr/lib/udev/rules.d/99-recovery.rules'),
        'RUN+="/bin/sh -i"\n',
      ),
  ],
  [
    'changed optional package rule',
    tree =>
      writeFile(
        join(tree, 'usr/lib/udev/rules.d/55-dm.rules'),
        'RUN+="/bin/sh -i"\n',
      ),
  ],
  [
    'missing required udev rule',
    tree => rm(join(tree, 'usr/lib/udev/rules.d/60-persistent-storage.rules')),
  ],
  [
    'symlink udev rule',
    async tree => {
      const path = join(tree, 'usr/lib/udev/rules.d/80-drivers.rules')
      await rm(path)
      await symlink('/bin/sh', path)
    },
  ],
  [
    'symlink rule ancestor',
    async tree => {
      const library = join(tree, 'usr/lib')
      await rm(library, { recursive: true })
      await symlink('/usr/lib', library)
    },
  ],
  ['unexpected lib alias', tree => symlink('/usr/lib', join(tree, 'lib'))],
  ['independent lib directory', tree => mkdir(join(tree, 'lib'))],
  ...['etc', 'run', 'usr/local/lib'].map(prefix => [
    `alternate ${prefix} rules`,
    async tree => {
      const directory = join(tree, prefix, 'udev/rules.d')
      await mkdir(directory, { recursive: true })
      await writeFile(
        join(directory, '50-udev-default.rules'),
        'RUN+="/bin/sh -i"\n',
      )
    },
  ]),
  [
    'symlink alternate rules ancestor',
    tree => symlink('/etc', join(tree, 'etc')),
  ],
]) {
  test(`${name} fails before any transformation`, async t => {
    const { tree } = await copyFixture(t)
    await mutate(tree)
    const before = await snapshot(tree)
    await assert.rejects(harden(tree))
    assert.deepEqual(await snapshot(tree), before)
  })
}

for (const shell of shells) {
  for (const status of [0, 1])
    test(`${shell.name}: panic cannot fall through when reboot returns ${status}`, async t => {
      const { temp, tree } = await copyFixture(t)
      await harden(tree)
      const fail = join(tree, 'scripts/fde-fail')
      const functions = (
        await readFile(join(tree, 'scripts/functions'), 'utf8')
      ).replace('. /scripts/fde-fail', `. '${fail}'`)
      const relocated = join(temp, 'functions')
      await writeFile(relocated, functions)
      await assertStops(
        temp,
        `. '${relocated}'`,
        'panic "failure"',
        shell,
        status,
      )
    })

  test(`${shell.name}: break requests never invoke panic or a shell`, async t => {
    const { temp, tree } = await copyFixture(t)
    await harden(tree)
    const functions = (
      await readFile(join(tree, 'scripts/functions'), 'utf8')
    ).replace('. /scripts/fde-fail', `. '${join(tree, 'scripts/fde-fail')}'`)
    const relocated = join(temp, 'functions')
    await writeFile(relocated, functions)
    const result = spawnSync(
      shell.program,
      [
        ...shell.args,
        '-ec',
        `. '${relocated}'; panic() { exit 99; }; break=top; maybe_break top; printf done`,
      ],
      { encoding: 'utf8' },
    )
    assert.equal(result.status, 0)
    assert.equal(result.stdout, 'done')
  })

  test(`${shell.name}: a failed provider stops the parent instead of continuing its ordering file`, async t => {
    const { temp, tree } = await copyFixture(t)
    await harden(tree)
    const directory = join(temp, 'providers')
    await mkdir(directory)
    await writeFile(
      join(directory, 'ORDER'),
      'false\nprintf after-failed-provider\n',
    )
    const functions = (
      await readFile(join(tree, 'scripts/functions'), 'utf8')
    ).replace('. /scripts/fde-fail', `. '${join(tree, 'scripts/fde-fail')}'`)
    const relocated = join(temp, 'functions')
    await writeFile(relocated, functions)
    await assertStops(
      temp,
      `. '${relocated}'; trap 'fde_fail' EXIT`,
      `run_scripts '${directory}'`,
      shell,
    )
  })

  test(`${shell.name}: missing init reaches the terminal failure handler, never another init`, async t => {
    const { temp, tree } = await copyFixture(t)
    await harden(tree)
    const init = await readFile(join(tree, 'init'), 'utf8')
    const validation = init.slice(
      init.indexOf('# No init on rootmount'),
      init.indexOf('maybe_break init'),
    )
    assert.ok(validation.includes('panic'))
    await assertStops(
      temp,
      `. '${join(tree, 'scripts/fde-fail')}'; panic() { fde_fail; }; validate_init() { return 1; }; init=/sbin/init`,
      validation,
      shell,
    )
  })
}
