// The caller owns a private generated tree and discards it on failure.
import {
  lstat,
  readdir,
  readFile,
  readlink,
  realpath,
  rm,
  writeFile,
} from 'node:fs/promises'
import { dirname, isAbsolute, join, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawnSync } from 'node:child_process'

import { replaceOnce, requireReviewedContext } from './fde-reviewed-source.mjs'
import { requireNativeHookFunctions } from './fde-initramfs-producer.mjs'
import {
  checkedDirectory,
  filesUnder,
  kernelDirectory,
} from './fde-initramfs-paths.mjs'

const bootMode = (
  await readFile(new URL('./fde-boot-mode', import.meta.url), 'utf8')
).trim()

const inputs = {
  init: [
    '0a9bb34973c78987922b57f010e99c36ddf102e8a5a2fd198385566539f5d6d5',
    'patch',
  ],
  'scripts/functions': [
    'b328b49756e00ec7c692dcdd28526831def5e9e0d7a0d0963087b450c2db4743',
    'patch',
  ],
  'scripts/local': [
    'e4af112fd0d528fcb9b6094a9a2dd529b8936310d6939f7272299951b9a27448',
    'keep',
  ],
  'scripts/nfs': [
    '6d8787c085c49528afe7c3766a26ffa9196a5fcf90079984a52e5b1e8fc4917d',
    'keep',
  ],
  'scripts/live': [
    'd290a48efe77d0f43cca08d9b7f8ab9a92704a4fc7801ae15c5d2fe2c9aca642',
    'drop',
  ],
  'scripts/startos': [
    '89ac1127c32a8aa194e2f24ad58dbf16e8565416edf15bd1dce051c33c5790de',
    'drop',
  ],
  'scripts/init-top/ORDER': [
    'd550aefa50c796756833fe9b39441f55e50b87939f7451d381deb8a93d53cb7a',
    'order',
  ],
  'scripts/init-top/all_generic_ide': [
    '59697cff80663c30132578d3fc6128692ff15826b4e32df3468e85db03ba3b93',
    'keep',
  ],
  'scripts/init-top/blacklist': [
    'e3fd2f67b92756f053ee3045c45f4d40ba957bf310f2ca64c11f80f6f25604a1',
    'keep',
  ],
  'scripts/init-top/keymap': [
    '2d04d7d7705d1c06cb9fff4c5290c02e719120e43ac65c387f00d8ef37b2d4cf',
    'drop',
  ],
  'scripts/init-top/udev': [
    '5893d2c4c9426b2d6166586cb5bd1852be8fa22ba2984b9e1b2ca15f8b9cca5f',
    'keep',
  ],
  'scripts/init-bottom/ORDER': [
    '0bbd4b986672e719d7f21f02c8cc25bf6c09c4756088a1b115ebb80b3a64cde1',
    'order',
  ],
  'scripts/init-bottom/udev': [
    '5a7595d79328000420c9f641362af1ef15aa5b1bf6ce0114588b49e13f7949a6',
    'keep',
  ],
  'scripts/local-premount/ORDER': [
    '4d556ffbba296828f00fb271dc561b4d2e0b2177f24e0bca5a9806e3bdfb8cb3',
    'order',
  ],
  'scripts/local-premount/btrfs': [
    '642807af4437aa1c88a764933169a3907b90e8bff4714191b65d56b6a517f49d',
    'keep',
  ],
  'scripts/local-premount/resume': [
    'e8b4ec9ba6e78965e9c054cd296291b3ed1441b3eef25c9c94b8a37afee53b3a',
    'drop',
  ],
  'conf/initramfs.conf': [
    'fc29caea4dbdbfd8891d3f7a64f136ba1a0c427fece0993063cb46846a105bdf',
    'keep',
  ],
  'conf/conf.d/cryptsetup': [
    '4c9a78dc93bb9ecf3422288adac6a48bb59f1be7fa4e20004263457697878bb3',
    'drop',
  ],
  'conf/modules': [
    '4066749d0106de5e2d230b91dadcfd56fc7866661222e94b47b600683ddea538',
    'keep',
  ],
  'usr/lib/udev/rules.d/50-firmware.rules': [
    '37895a6ae81ab0e8753eb97a7c11b7cf578549e06fbbf2d6b192d109bc2cd9a3',
    'keep',
  ],
  'usr/lib/udev/rules.d/50-udev-default.rules': [
    '2fc7f00aa36ea57400d14b9a765b7d26f39a004f9bf666f69283d9aa5aaa34d2',
    'patch',
  ],
  'usr/lib/udev/rules.d/55-dm.rules': [
    '106ec258d978828312aca9c5730a674811542e74f403d9971c07ff3195d857e5',
    'keep',
    false,
  ],
  'usr/lib/udev/rules.d/60-block.rules': [
    '93ecc4fab045601b0eb16ef1d74eb82cd1d618fffb3de4bd6076b533f72b406a',
    'keep',
  ],
  'usr/lib/udev/rules.d/60-persistent-storage.rules': [
    '6ea07cd61b4a221ea219baee6a0f4ca1524f2920a172485b05cbdbecac0d7b9c',
    'keep',
  ],
  'usr/lib/udev/rules.d/60-persistent-storage-dm.rules': [
    'a19445a6c7d2b79858510c38b3409a5a419d40f5d3f78b009be08c8551a52fbc',
    'keep',
    false,
  ],
  'usr/lib/udev/rules.d/64-btrfs.rules': [
    'a4bc67740cd0988415907b0fdb5e9cd955be2626b5095b55c7eed4aa4e99bb53',
    'keep',
    false,
  ],
  'usr/lib/udev/rules.d/64-btrfs-dm.rules': [
    'ba9da67a41dce05c3b9606dac86fd514e41635b0280371df1f46046a3ae485ee',
    'keep',
    false,
  ],
  'usr/lib/udev/rules.d/64-btrfs-zoned.rules': [
    'dfa477a4af0399a5da49fcb1ffc6c5fbb4da6bf2ba69cbab91aef77c07fb2e90',
    'keep',
    false,
  ],
  'usr/lib/udev/rules.d/71-seat.rules': [
    'b57a64ab6c973d0b734dea808e6caf9a49b40739c7c7649784d09bd6d90c36d3',
    'keep',
    false,
  ],
  'usr/lib/udev/rules.d/73-special-net-names.rules': [
    'fcbf4bbd63bec2c63f9b9d11b90cac3a137f37cf69692033108426d44a8e6cea',
    'patch',
  ],
  'usr/lib/udev/rules.d/75-net-description.rules': [
    '877959a4597e1e9d30158773b7977f44b136c88a65225c7abe3cd52f4ecd096f',
    'keep',
  ],
  'usr/lib/udev/rules.d/80-net-setup-link.rules': [
    '7056ebc8ef611d0f61b2ac01e20fa8cca035f957e58fd60600e3eec9bab927c2',
    'keep',
  ],
  'usr/lib/udev/rules.d/80-drivers.rules': [
    '0941cea0be8d3564556b5787a87cf55971fd3e46e19c56ad68a4205e8ad3a560',
    'keep',
  ],
  'usr/lib/udev/rules.d/95-dm-notify.rules': [
    'd24e725fc78306ff2355eb91e4eebcc7dc9388d54dc27d20d6fe6f695054d536',
    'keep',
    false,
  ],
}

const sourceHookConfigurations = {
  'usr/share/initramfs-tools/conf-hooks.d/busybox':
    '0d86ad66470b73e020ca349e66262c6cbab1f2f1d0c88836ca28f8e4b8e6828b',
  'usr/share/initramfs-tools/conf-hooks.d/cryptsetup':
    '52278458942c4e152127b687dba7bf1ca96b03d8fa692fa4f479a4ff0d87bdb1',
}

export const bootModules = ['overlay', 'btrfs', 'dm_verity', 'loop', 'squashfs']

function requireReviewedInput(path, bytes) {
  if (path === 'conf/arch.conf') {
    if (!/^DPKG_ARCH=[a-z0-9-]+\n$/.test(bytes.toString()))
      throw Error('Unsupported architecture configuration')
  } else {
    requireReviewedContext(
      bytes,
      inputs[path]?.[0] ?? sourceHookConfigurations[path],
      path,
    )
  }
}

function replaceRange(text, first, last, replacement) {
  const start = text.indexOf(first)
  const end = text.indexOf(last, start)
  if (
    start < 0 ||
    end < start ||
    text.indexOf(first, start + 1) >= 0 ||
    text.indexOf(last, end + 1) >= 0
  ) {
    throw Error('Unsupported generated context')
  }
  return text.slice(0, start) + replacement + text.slice(end)
}

async function optionalFilesUnder(root, directory) {
  try {
    await lstat(join(root, directory))
  } catch (error) {
    if (error.code === 'ENOENT') return []
    throw error
  }
  return filesUnder(root, directory)
}

async function readOriginalSource(root, path) {
  await checkedDirectory(root, dirname(path))
  if (!(await lstat(join(root, path))).isFile())
    throw Error(`Unsupported source file type: ${path}`)
  return readFile(join(root, path))
}

async function admitRuntimeDirectory(root, directory) {
  const paths = await optionalFilesUnder(root, directory)
  for (const path of paths) {
    const key = `scripts/${path.slice(directory.length + 1)}`
    if (inputs[key]?.[1] === 'order')
      throw Error(`Generated ordering is not a runtime source: ${key}`)
    requireReviewedInput(key, await readFile(join(root, path)))
  }
  return paths
}

/** Original providers and configuration remain stable through native generation. */
export async function admitRuntimeSources(sourceRoot, configDirectory) {
  const shared = 'usr/share/initramfs-tools/scripts'
  const providers = await admitRuntimeDirectory(sourceRoot, shared)
  if (!providers.includes(`${shared}/functions`))
    throw Error('Missing native runtime functions source')
  await admitRuntimeDirectory(configDirectory, 'scripts')
}

/** The caller excludes writers to admitted sources until native consumption ends. */
export async function admitInitramfsSources(sourceRoot, configDirectory) {
  await admitRuntimeSources(sourceRoot, configDirectory)
  requireNativeHookFunctions(
    await readOriginalSource(
      sourceRoot,
      'usr/share/initramfs-tools/hook-functions',
    ),
  )
  requireReviewedInput(
    'conf/initramfs.conf',
    await readOriginalSource(configDirectory, 'initramfs.conf'),
  )
  for (const [root, directory, prefix] of [
    [sourceRoot, 'usr/share/initramfs-tools/conf.d', 'conf/conf.d'],
    [configDirectory, 'conf.d', 'conf/conf.d'],
    [
      sourceRoot,
      'usr/share/initramfs-tools/conf-hooks.d',
      'usr/share/initramfs-tools/conf-hooks.d',
    ],
  ]) {
    for (const path of await optionalFilesUnder(root, directory)) {
      const key = `${prefix}/${path.slice(directory.length + 1)}`
      requireReviewedInput(key, await readFile(join(root, path)))
    }
  }
}

async function rejectAlternateRules(root) {
  for (const directory of [
    'etc/udev/rules.d',
    'run/udev/rules.d',
    'usr/local/lib/udev/rules.d',
  ]) {
    try {
      if ((await filesUnder(root, directory)).length)
        throw Error(`Unsupported alternate udev rules: ${directory}`)
    } catch (error) {
      if (error.code !== 'ENOENT') throw error
    }
  }
  let lib
  try {
    lib = await lstat(join(root, 'lib'))
  } catch (error) {
    if (error.code !== 'ENOENT') throw error
  }
  if (
    !lib ||
    !lib.isSymbolicLink() ||
    (await readlink(join(root, 'lib'))) !== 'usr/lib'
  )
    throw Error('Unsupported initramfs lib alias')
}

function nativeAuditOutput(executable, args) {
  const query = spawnSync(executable, args, {
    encoding: 'utf8',
    env: { PATH: process.env.PATH, LC_ALL: 'C' },
    maxBuffer: 16 * 1024 * 1024,
  })
  if (query.error || query.status !== 0 || query.stderr)
    throw Error(
      `Cannot audit module configuration: ${query.error ?? query.stderr}`,
    )
  return query.stdout
}

async function requireCopiedProvider(root, target, source) {
  const base = await realpath(root)
  const path = await realpath(join(base, target))
  const within = relative(base, path)
  if (within === '..' || within.startsWith('../') || isAbsolute(within))
    throw Error(`Module provider escapes the generated tree: ${target}`)
  if (!(await lstat(path)).isFile())
    throw Error(`Unsupported module provider: ${target}`)
  if (!(await readFile(path)).equals(await readFile(source)))
    throw Error(
      `Copied module provider differs from the native query: ${target}`,
    )
}

async function admittedModuleProviders(root) {
  const executable = nativeAuditOutput('/bin/sh', [
    '-c',
    'command -v modprobe',
  ]).trim()
  const architecture = nativeAuditOutput('dpkg', [
    '--print-architecture',
  ]).trim()
  const libraries = nativeAuditOutput('dpkg-query', [
    '-L',
    `libkmod2:${architecture}`,
  ])
    .split('\n')
    .filter(path => path.endsWith('/libkmod.so.2'))
  if (libraries.length !== 1)
    throw Error('Cannot identify the native libkmod query provider')
  const library = libraries[0]
  await requireCopiedProvider(root, 'usr/sbin/modprobe', executable)
  await requireCopiedProvider(root, library.slice(1), library)
  return { executable, library }
}

/** Returns query/configuration paths after CLI/shared-library admission; inputs remain stable through use. */
export async function admitModuleConfiguration(root) {
  const { path: modulePath, version } = await kernelDirectory(root)
  const moduleRoot = join(root, modulePath)
  for (const name of ['modules.softdep', 'modules.weakdep']) {
    try {
      if (!(await lstat(join(moduleRoot, name))).isFile())
        throw Error(`Unsupported module configuration: ${name}`)
    } catch (error) {
      if (error.code !== 'ENOENT') throw error
    }
  }
  const directories = [
    'etc/modprobe.d',
    'run/modprobe.d',
    'usr/local/lib/modprobe.d',
    'usr/lib/modprobe.d',
  ]
  for (const directory of directories) {
    try {
      await filesUnder(root, directory)
    } catch (error) {
      if (error.code !== 'ENOENT') throw error
    }
  }
  const { executable, library } = await admittedModuleProviders(root)
  const paths = directories.map(directory => join(root, directory))
  const configuration = nativeAuditOutput(executable, [
    '--showconfig',
    `--dirname=${resolve(root)}`,
    `--set-version=${version}`,
    ...paths.flatMap(directory => ['--config', directory]),
  ])
  const sharedConfiguration = nativeAuditOutput('python3', [
    '-I',
    fileURLToPath(new URL('./fde-module-config.py', import.meta.url)),
    library,
    moduleRoot,
    ...paths,
  ])
  if (/^(install|remove) /m.test(`${configuration}\n${sharedConfiguration}`))
    throw Error(
      'Pre-verification module configuration invokes external commands',
    )
  return { executable, library, paths }
}

/** Hardens reviewed scripts, rules and module configuration, not the complete executable closure. */
export async function harden(root) {
  await rejectAlternateRules(root)
  const files = [
    'init',
    ...(await filesUnder(root, 'scripts')),
    ...(await filesUnder(root, 'conf')),
    ...(await filesUnder(root, 'usr/lib/udev/rules.d')),
  ]
  const original = new Map()
  for (const path of files) {
    if (!(await lstat(join(root, path))).isFile())
      throw Error(`Unsupported generated file type: ${path}`)
    const bytes = await readFile(join(root, path))
    requireReviewedInput(path, bytes)
    original.set(path, bytes.toString())
  }
  for (const [path, [, , required = true]] of Object.entries(inputs)) {
    if (required && !original.has(path))
      throw Error(`Missing generated context: ${path}`)
  }
  if (!original.has('conf/arch.conf'))
    throw Error('Missing architecture configuration')
  await admitModuleConfiguration(root)

  const output = new Map(original)
  let init = output.get('init')
  init = replaceOnce(
    init,
    '#!/bin/sh\n',
    "#!/bin/sh\n. /scripts/fde-fail || exit 1\nset -e\ntrap 'fde_fail' EXIT\n",
  )
  init = replaceOnce(
    init,
    'for conf in conf/conf.d/*; do\n\t[ -f "${conf}" ] && . "${conf}"\ndone\n',
    '',
  )
  init = replaceRange(
    init,
    '# Default to BOOT=local',
    'if [ -n "${noresume}" ]',
    `set +x\nBOOT=${bootMode}\nIP=none\nnoresume=y\nresume=\ninit=/sbin/init\n\n`,
  )
  init = replaceRange(
    init,
    'if read_fstab_entry /usr; then',
    '# Mount cleanup',
    '',
  )
  init = replaceRange(
    init,
    '# Check init is really there',
    '# No init on rootmount',
    '',
  )
  output.set('init', init)
  let functions = output.get('scripts/functions')
  functions =
    '. /scripts/fde-fail\n' +
    replaceRange(
      functions,
      'panic()\n{',
      '# For boot time only;',
      'panic()\n{\n\tfde_fail\n}\n\nmaybe_break()\n{\n\t:\n}\n\n',
    )
  functions = replaceOnce(
    functions,
    '\t. "${initdir}/ORDER"\n}',
    '\t. "${initdir}/ORDER"\n\treturn 0\n}',
  )
  output.set('scripts/functions', functions)
  const dropped = Object.entries(inputs)
    .filter(([, [, action]]) => action === 'drop')
    .map(([path]) => path)
  for (const path of dropped) output.delete(path)
  for (const [path, [, action]] of Object.entries(inputs)) {
    if (action !== 'order') continue
    let order = output.get(path)
    for (const removed of dropped.filter(
      removed => dirname(removed) === dirname(path),
    )) {
      order = replaceOnce(
        order,
        `/${removed} "$@"\n[ -e /conf/param.conf ] && . /conf/param.conf\n`,
        '',
      )
    }
    output.set(
      path,
      order.replaceAll('[ -e /conf/param.conf ] && . /conf/param.conf\n', ''),
    )
  }
  const defaults = 'usr/lib/udev/rules.d/50-udev-default.rules'
  output.set(
    defaults,
    replaceOnce(
      output.get(defaults),
      '# run a command on remove events\nACTION=="remove", ENV{REMOVE_CMD}!="", RUN+="$env{REMOVE_CMD}"\n',
      '',
    ),
  )
  const names = 'usr/lib/udev/rules.d/73-special-net-names.rules'
  output.set(names, output.get(names).split('# ibmveth devices')[0])
  output.set('conf/modules', bootModules.join('\n') + '\n')
  const here = dirname(fileURLToPath(import.meta.url))
  output.set(
    'scripts/fde-fail',
    await readFile(join(here, 'fde-initramfs-fail'), 'utf8'),
  )
  output.set(
    `scripts/${bootMode}`,
    await readFile(join(here, 'startos-fde-initramfs-module'), 'utf8'),
  )
  for (const [path, text] of output) {
    if (path.startsWith('usr/lib/udev/rules.d/')) continue
    const syntax = spawnSync('sh', ['-n'], { input: text, encoding: 'utf8' })
    if (syntax.status !== 0)
      throw Error(`Invalid hardened shell: ${path}: ${syntax.stderr}`)
  }
  for (const path of dropped) await rm(join(root, path))
  for (const [path, text] of output) await writeFile(join(root, path), text)
}
