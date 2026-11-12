import { constants } from 'node:fs'
import {
  access,
  lstat,
  mkdir,
  readFile,
  readdir,
  realpath,
  writeFile,
} from 'node:fs/promises'
import {
  basename,
  dirname,
  isAbsolute,
  join,
  relative,
  resolve,
} from 'node:path'

import { replaceOnce, requireReviewedContext } from './fde-reviewed-source.mjs'

const hooks = {
  fsck: ['53593d50a0be41f95bb191b941857e7cfb6050bab2e6778e20dacf6fe8207081'],
  kmod: ['23eeee72b630029cc717ccc4c4e927fa35f55e3c30e66c1dd85713fe2100c7f1'],
  'klibc-utils': [
    '56e06e437c8b05f066d3f86f9af7b49425b27949164aaf83890dbe154ebe42ba',
  ],
  thermal: ['8000b165e2c6ef915fc8d7d6bf2af07942d461cb240e0590c5d1f99a56d8b14f'],
  'zz-busybox': [
    '41b934003c3683a39d0aba0fee8fc4febffabce90498d2e126c1648b55ec64ae',
  ],
  udev: ['a1aab3a02b3ec52fca3f6287f3b7185cf3d49756ca7df6de4fcd5d2a04a49a86'],
  dmsetup: ['d7aba5b55840b07c8b885d7fcd18c6e5aa9b9ed0c266968e06d56d3dde2743cd'],
  btrfs: ['695a9e48c9af29c0e3802ce0defaab3e20fb5b36a561397174cf215ffafc60f5'],
  intel_microcode: [
    'ccbbf42e08a0eed96c9fd6fdc2d4e223e070d99085a67170696c8a3abcc31eec',
    false,
  ],
  amd64_microcode: [
    '6f8f38c3bbf7ea6bed606cb2c2757e40d22d81397c626d8118983809b097e72f',
    false,
  ],
}

const excluded = new Set([
  'keymap',
  'resume',
  'fuse',
  'cryptgnupg',
  'cryptgnupg-sc',
  'cryptkeyctl',
  'cryptopensc',
  'cryptpassdev',
  'cryptroot',
  'cryptroot-unlock',
])

const microcodeConfigs = {
  intel_microcode: [
    'intel-microcode',
    '72ae73b1c9a114d554bcd9f8eec13e7f40761b5ca18f6c3d29e4e23eebe966c9',
  ],
  amd64_microcode: [
    'amd64-microcode',
    '10fb3bfc2f6ae15152e096b89a7550bc4dd395bedff8d9e070d308e77d4adb9b',
  ],
}

function reviewedHook(name, bytes) {
  const original = bytes.toString()
  let context = original
  if (name === 'amd64_microcode') {
    const timestamps = [...context.matchAll(/^CHANGELOG_TS=[0-9]+$/gm)]
    if (timestamps.length !== 1)
      throw Error('Unsupported AMD microcode timestamp')
    context = replaceOnce(context, timestamps[0][0], 'CHANGELOG_TS=0')
  }
  requireReviewedContext(Buffer.from(context), hooks[name][0], `hook ${name}`)
  if (name === 'intel_microcode') {
    return replaceOnce(
      replaceOnce(
        replaceOnce(
          original,
          'echo "E: intel-microcode: cannot run iucode_tool!" >&2\n\texit 0',
          'echo "E: intel-microcode: cannot run iucode_tool!" >&2\n\texit 1',
        ),
        'echo "E: intel-microcode: unsupported kernel version!" >&2\n       exit 0',
        'echo "E: intel-microcode: unsupported kernel version!" >&2\n       exit 1',
      ),
      'echo "E: intel-microcode: failed to create or prepend the early initramfs to the initramfs" >&2\n\n:',
      'echo "E: intel-microcode: failed to create or prepend the early initramfs to the initramfs" >&2\n\nexit 1',
    )
  }
  if (name === 'amd64_microcode') {
    return replaceOnce(
      replaceOnce(
        original,
        'echo "E: amd64-microcode: unsupported kernel version!" >&2\n    exit 0',
        'echo "E: amd64-microcode: unsupported kernel version!" >&2\n    exit 1',
      ),
      'echo "E: amd64-microcode: failed to create or prepend the early initramfs to the initramfs" >&2\n    exit 0',
      'echo "E: amd64-microcode: failed to create or prepend the early initramfs to the initramfs" >&2\n    exit 1',
    )
  }
  return original
}

/**
 * Copies admitted native hooks into a fresh private directory; source providers remain unchanged.
 * The caller keeps source helpers/configuration and candidate files stable through generation.
 * Unknown hooks reject; credential/resume/keymap/FUSE hooks do not enter the FDE profile.
 * Hooks execute in their admitted source namespace; ordering/firmware/archives remain native.
 */
export async function prepareNativeHooks(sourceRoot, targetDirectory) {
  const source = await realpath(
    join(sourceRoot, 'usr/share/initramfs-tools/hooks'),
  )
  const requested = resolve(targetDirectory)
  const target = join(await realpath(dirname(requested)), basename(requested))
  const within = relative(source, target)
  if (
    within === '' ||
    (within !== '..' && !within.startsWith('../') && !isAbsolute(within))
  )
    throw Error('Private hook directory is inside source hooks')
  const output = new Map()
  for (const entry of await readdir(source, { withFileTypes: true })) {
    if (excluded.has(entry.name)) continue
    if (!hooks[entry.name])
      throw Error(`Unsupported native hook: ${entry.name}`)
    const path = join(source, entry.name)
    if (!entry.isFile())
      throw Error(`Unsupported native hook file: ${entry.name}`)
    try {
      await access(path, constants.X_OK)
    } catch {
      throw Error(`Unsupported native hook file: ${entry.name}`)
    }
    output.set(entry.name, reviewedHook(entry.name, await readFile(path)))
    const config = microcodeConfigs[entry.name]
    if (config) {
      const path = join(sourceRoot, 'etc/default', config[0])
      try {
        if (!(await lstat(path)).isFile())
          throw Error(`Unsupported microcode configuration: ${config[0]}`)
        requireReviewedContext(await readFile(path), config[1], config[0])
      } catch (error) {
        if (error.code !== 'ENOENT') throw error
      }
    }
  }
  for (const [name, [, required = true]] of Object.entries(hooks))
    if (required && !output.has(name))
      throw Error(`Missing native hook: ${name}`)
  await mkdir(target, { mode: 0o700 })
  for (const [name, text] of output)
    await writeFile(join(target, name), text, { flag: 'wx', mode: 0o755 })
}
