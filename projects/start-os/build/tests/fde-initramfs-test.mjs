import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { spawn, spawnSync } from 'node:child_process'
import {
  cp,
  glob,
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
import { replaceOnce } from '../lib/scripts/fde-reviewed-source.mjs'

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
  await cp(fixture, tree, { recursive: true, verbatimSymlinks: true })
  return { temp, tree }
}

async function copiedKmodLibrary(tree) {
  const paths = await Array.fromAsync(
    glob('usr/lib/**/libkmod.so.2', { cwd: tree }),
  )
  assert.equal(paths.length, 1)
  return join(tree, paths[0])
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
    '71-seat.rules',
    '95-dm-notify.rules',
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
    '71-seat.rules',
    '95-dm-notify.rules',
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

test('module options, aliases and dependencies preserve platform parsing and priority', async t => {
  const { tree } = await copyFixture(t)
  const high = join(tree, 'etc/modprobe.d')
  const low = join(tree, 'usr/lib/modprobe.d')
  await mkdir(high, { recursive: true })
  await writeFile(
    join(low, 'priority.conf'),
    'install fde_fixture /bin/sh -i\n',
  )
  const configuration =
    'options fde_fixture label="install is data"\nalias fde_alias fde_fixture\nblacklist fde_unused\nsoftdep fde_fixture pre: fde_before post: fde_after\nweakdep fde_fixture fde_optional\n'
  await writeFile(join(high, 'priority.conf'), configuration)
  await harden(tree)
  assert.equal(
    await readFile(join(high, 'priority.conf'), 'utf8'),
    configuration,
  )
  assert.equal(
    await readFile(join(low, 'priority.conf'), 'utf8'),
    'install fde_fixture /bin/sh -i\n',
  )
})

for (const [name, mutate, error] of [
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
  ...['71-seat.rules', '95-dm-notify.rules'].map(name => [
    `changed native-hook rule ${name}`,
    tree =>
      writeFile(
        join(tree, 'usr/lib/udev/rules.d', name),
        'RUN+="/bin/sh -i"\n',
      ),
    /Unsupported/,
  ]),
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
  [
    'unexpected lib alias',
    async tree => {
      await rm(join(tree, 'lib'))
      await symlink('/usr/lib', join(tree, 'lib'))
    },
  ],
  [
    'independent lib directory',
    async tree => {
      await rm(join(tree, 'lib'))
      await mkdir(join(tree, 'lib'))
    },
  ],
  ['missing lib alias', tree => rm(join(tree, 'lib'))],
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
  ...['etc', 'run', 'usr/local/lib', 'usr/lib'].map(prefix => [
    `module install command in ${prefix}`,
    async tree => {
      const directory = join(tree, prefix, 'modprobe.d')
      await mkdir(directory, { recursive: true })
      await writeFile(
        join(directory, 'recovery.conf'),
        ['  install fde_fixture \\', ' /bin/sh -i', ''].join('\n'),
      )
    },
    /invokes external commands/,
  ]),
  [
    'module removal command',
    tree =>
      writeFile(
        join(tree, 'usr/lib/modprobe.d/recovery.conf'),
        'remove fde_fixture /bin/sh -i\n',
      ),
    /invokes external commands/,
  ],
  [
    'malformed module configuration',
    tree =>
      writeFile(
        join(tree, 'usr/lib/modprobe.d/recovery.conf'),
        'unknown fde_fixture value\n',
      ),
    /Cannot audit module configuration/,
  ],
  ...['modules.softdep', 'modules.weakdep'].map(name => [
    `module command in ${name}`,
    async tree => {
      const directory = join(tree, 'usr/lib/modules')
      const [version] = await readdir(directory)
      await writeFile(
        join(directory, version, name),
        'install fde_fixture /bin/sh -i\n',
      )
    },
    /invokes external commands/,
  ]),
  [
    'symlink module configuration',
    tree =>
      symlink('/etc/passwd', join(tree, 'usr/lib/modprobe.d/recovery.conf')),
  ],
  [
    'symlink module metadata',
    async tree => {
      const directory = join(tree, 'usr/lib/modules')
      const [version] = await readdir(directory)
      await symlink('/etc/passwd', join(directory, version, 'modules.softdep'))
    },
  ],
  [
    'multiple kernel versions',
    tree => mkdir(join(tree, 'usr/lib/modules/unselected-kernel')),
  ],
  [
    'missing runtime modprobe',
    tree => rm(join(tree, 'usr/sbin/modprobe')),
    /ENOENT/,
  ],
  [
    'changed runtime modprobe',
    tree => writeFile(join(tree, 'usr/sbin/modprobe'), 'changed'),
    /Copied module provider differs/,
  ],
  [
    'missing runtime libkmod',
    async tree => rm(await copiedKmodLibrary(tree)),
    /ENOENT/,
  ],
  [
    'changed runtime libkmod',
    async tree => writeFile(await copiedKmodLibrary(tree), 'changed'),
    /Copied module provider differs/,
  ],
  [
    'runtime library link to host provider',
    async tree => {
      const path = await copiedKmodLibrary(tree)
      const source = spawnSync('dpkg-query', ['-L', 'libkmod2'], {
        encoding: 'utf8',
      })
        .stdout.split('\n')
        .find(path => path.endsWith('/libkmod.so.2'))
      assert.ok(source)
      await rm(path)
      await symlink(source, path)
    },
    /Module provider escapes/,
  ],
  [
    'environment-suppressed parser errors',
    async (tree, t) => {
      const before = process.env.KMOD_LOG
      t.after(() => {
        if (before === undefined) delete process.env.KMOD_LOG
        else process.env.KMOD_LOG = before
      })
      process.env.KMOD_LOG = '0'
      await writeFile(
        join(tree, 'usr/lib/modprobe.d/recovery.conf'),
        'unknown fde_fixture value\n',
      )
    },
    /Cannot audit module configuration/,
  ],
  [
    'environment-selected query bypass',
    async (tree, t) => {
      const before = process.env.MODPROBE_OPTIONS
      t.after(() => {
        if (before === undefined) delete process.env.MODPROBE_OPTIONS
        else process.env.MODPROBE_OPTIONS = before
      })
      process.env.MODPROBE_OPTIONS = '--version'
      await writeFile(
        join(tree, 'usr/lib/modprobe.d/recovery.conf'),
        'install fde_fixture /bin/sh -i\n',
      )
    },
    /invokes external commands/,
  ],
]) {
  test(`${name} fails before any transformation`, async t => {
    const { tree } = await copyFixture(t)
    await mutate(tree, t)
    const before = await snapshot(tree)
    await assert.rejects(harden(tree), error)
    assert.deepEqual(await snapshot(tree), before)
  })
}

test('shared libkmod reports commands without executing or probing modules', async t => {
  const { temp, tree } = await copyFixture(t)
  const source = spawnSync('dpkg-query', ['-L', 'libkmod2'], {
    encoding: 'utf8',
  })
    .stdout.split('\n')
    .find(path => path.endsWith('/libkmod.so.2'))
  assert.ok(source)
  const sentinel = join(temp, 'command-executed')
  const configs = join(tree, 'usr/lib/modprobe.d')
  await writeFile(
    join(configs, 'commands.conf'),
    `install fde_fixture touch ${sentinel}\nremove another_fixture touch ${sentinel}\n`,
  )
  const [version] = await readdir(join(tree, 'usr/lib/modules'))
  const args = [
    '-I',
    fileURLToPath(
      new URL('../lib/scripts/fde-module-config.py', import.meta.url),
    ),
    source,
    join(tree, 'usr/lib/modules', version),
    configs,
  ]
  const options = {
    encoding: 'utf8',
    env: { PATH: process.env.PATH, LC_ALL: 'C' },
  }
  const result = spawnSync('python3', args, options)
  assert.equal(result.status, 0, result.stderr)
  assert.equal(result.stderr, '')
  assert.deepEqual(result.stdout.trim().split('\n'), [
    'install fde_fixture',
    'remove another_fixture',
  ])
  await assert.rejects(lstat(sentinel), { code: 'ENOENT' })
  await writeFile(join(configs, 'commands.conf'), 'unknown fde_fixture value\n')
  const malformed = spawnSync('python3', args, options)
  assert.match(malformed.stderr, /unknown/)
  await assert.rejects(lstat(sentinel), { code: 'ENOENT' })
})

test('native device wait rejects an absent device with a zero timeout', async () => {
  const device = `/dev/fde-absent-tpm-${process.pid}`
  await assert.rejects(lstat(device), { code: 'ENOENT' })
  const result = spawnSync(
    'udevadm',
    ['wait', '--initialized=no', '--timeout=0', device],
    { encoding: 'utf8' },
  )
  assert.notEqual(result.status, 0)
  assert.match(result.stderr, /Timed out/)
})

for (const shell of shells) {
  for (const status of [0, 1])
    test(`${shell.name}: TPM readiness ${status ? 'failure stops release' : 'pins only the preparation command transport'}`, async t => {
      const { temp, tree } = await copyFixture(t)
      await harden(tree)
      const module = await readFile(join(tree, 'scripts/startos-fde'), 'utf8')
      const declaration = module.match(/\tlocal tpm_device=.*\n/)
      assert.ok(declaration)
      const start = module.indexOf('\tudevadm wait ')
      const end = module.lastIndexOf('\n}')
      assert.ok(start > 0 && end > start)
      const environment = join(temp, 'command-environment')
      const parent = join(temp, 'parent-environment')
      const requests = join(temp, 'udev-request')
      const observation = replaceOnce(
        module.slice(start, end),
        '/usr/bin/startbox start-fde-prepare /startos "${rootmnt}"',
        `env > '${environment}'`,
      )
      const result = spawnSync(
        shell.program,
        [
          ...shell.args,
          '-ec',
          `
udevadm() { printf '%s\\n' "$@" > '${requests}'; return ${status}; }
panic() { exit 37; }
prepare() {
${declaration[0]}${observation}
}
prepare
printf '%s' "$TPM2TOOLS_TCTI" > '${parent}'
`,
        ],
        {
          encoding: 'utf8',
          env: { PATH: process.env.PATH, TPM2TOOLS_TCTI: 'cmd:untrusted' },
        },
      )
      assert.deepEqual((await readFile(requests, 'utf8')).trim().split('\n'), [
        'wait',
        '--initialized=no',
        '--timeout=30',
        '/dev/tpmrm0',
      ])
      if (status) {
        assert.equal(result.status, 37, result.stderr)
        await assert.rejects(lstat(environment), { code: 'ENOENT' })
        await assert.rejects(lstat(parent), { code: 'ENOENT' })
      } else {
        assert.equal(result.status, 0, result.stderr)
        assert.ok(
          (await readFile(environment, 'utf8'))
            .split('\n')
            .includes('TPM2TOOLS_TCTI=device:/dev/tpmrm0'),
        )
        assert.equal(await readFile(parent, 'utf8'), 'cmd:untrusted')
      }
    })

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
