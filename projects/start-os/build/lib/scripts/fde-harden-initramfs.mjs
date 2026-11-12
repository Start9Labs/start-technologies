#!/usr/bin/env node

// The caller owns a private generated tree and discards it on failure.
import { createHash } from 'node:crypto'
import { lstat, readdir, readFile, rm, writeFile } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawnSync } from 'node:child_process'

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
}

export const bootModules = ['overlay', 'btrfs', 'dm_verity', 'loop', 'squashfs']

function replaceOnce(text, before, after) {
  if (text.split(before).length !== 2)
    throw Error('Unsupported generated context')
  return text.replace(before, after)
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

async function filesUnder(root, relative) {
  if (!(await lstat(join(root, relative))).isDirectory())
    throw Error(`Unsupported generated directory: ${relative}`)
  const entries = []
  for (const entry of await readdir(join(root, relative), {
    withFileTypes: true,
  })) {
    const path = `${relative}/${entry.name}`
    if (entry.isDirectory()) entries.push(...(await filesUnder(root, path)))
    else if (entry.isFile()) entries.push(path)
    else throw Error(`Unsupported generated file type: ${path}`)
  }
  return entries
}

/** Hardens the reviewed initramfs-tools control plane, not the complete executable closure. */
export async function harden(root) {
  const files = [
    'init',
    ...(await filesUnder(root, 'scripts')),
    ...(await filesUnder(root, 'conf')),
  ]
  const original = new Map()
  for (const path of files) {
    if (!(await lstat(join(root, path))).isFile())
      throw Error(`Unsupported generated file type: ${path}`)
    const bytes = await readFile(join(root, path))
    if (path === 'conf/arch.conf') {
      if (!/^DPKG_ARCH=[a-z0-9-]+\n$/.test(bytes.toString()))
        throw Error('Unsupported architecture configuration')
    } else if (
      !inputs[path] ||
      createHash('sha256').update(bytes).digest('hex') !== inputs[path][0]
    ) {
      throw Error(`Unsupported generated context: ${path}`)
    }
    original.set(path, bytes.toString())
  }
  for (const path of Object.keys(inputs)) {
    if (!original.has(path)) throw Error(`Missing generated context: ${path}`)
  }
  if (!original.has('conf/arch.conf'))
    throw Error('Missing architecture configuration')

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
    'set +x\nBOOT=startos-fde\nIP=none\nnoresume=y\nresume=\ninit=/sbin/init\n\n',
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
  output.set('conf/modules', bootModules.join('\n') + '\n')
  const here = dirname(fileURLToPath(import.meta.url))
  output.set(
    'scripts/fde-fail',
    await readFile(join(here, 'fde-initramfs-fail'), 'utf8'),
  )
  output.set(
    'scripts/startos-fde',
    await readFile(join(here, 'startos-fde-initramfs-module'), 'utf8'),
  )
  for (const [path, text] of output) {
    const syntax = spawnSync('sh', ['-n'], { input: text, encoding: 'utf8' })
    if (syntax.status !== 0)
      throw Error(`Invalid hardened shell: ${path}: ${syntax.stderr}`)
  }
  for (const path of dropped) await rm(join(root, path))
  for (const [path, text] of output) await writeFile(join(root, path), text)
}

if (typeof import.meta.main !== 'boolean')
  throw Error('Node.js 24.2 or later is required')

if (import.meta.main) {
  if (process.argv.length !== 3)
    throw Error('Usage: fde-harden-initramfs.mjs <private-generated-tree>')
  await harden(resolve(process.argv[2]))
}
