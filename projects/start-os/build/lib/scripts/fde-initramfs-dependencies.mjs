import {
  lstat,
  mkdtemp,
  open,
  readFile,
  readdir,
  realpath,
  rm,
} from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, relative } from 'node:path'

import { requireNativeHookFunctions } from './fde-initramfs-producer.mjs'
import { publicSourcePath } from './fde-initramfs-providers.mjs'
import { runSource } from './fde-reviewed-source.mjs'

async function isElf(source) {
  const file = await open(source)
  try {
    const bytes = Buffer.alloc(4)
    await file.read(bytes, 0, bytes.length, 0)
    return bytes.equals(Buffer.from([0x7f, 0x45, 0x4c, 0x46]))
  } finally {
    await file.close()
  }
}

/**
 * Returns interpreter metadata after checking declared source ELF dependencies with the native loader.
 * The source is already authenticated; never supply a candidate.
 */
export async function inspectSourceElf(source) {
  if (!(await isElf(source))) return
  const { stdout } = await runSource('/usr/bin/readelf', [
    '--program-headers',
    '--dynamic',
    '--wide',
    source,
  ])
  if (/\(NEEDED\)/.test(stdout)) {
    // Native copy_exec does not propagate ldd failures.
    const trace = await runSource('/usr/bin/ldd', [source])
    if (`${trace.stdout}\n${trace.stderr}`.includes('not found'))
      throw Error(`Source ELF dependency is unresolved: ${source}`)
  }
  return [
    ...stdout.matchAll(/\[Requesting program interpreter: ([^\]\n]+)\]/g),
  ].map(match => match[1])
}

/**
 * Checks native copy_exec-selected ELF dependencies and interpreter paths after provider admission.
 * Source inputs and candidate remain stable. Firmware, loader configuration and runtime plugins retain separate obligations.
 */
export async function admitNativeDependencies(root, sources) {
  root = await realpath(root)
  const functions = await readFile('/usr/share/initramfs-tools/hook-functions')
  requireNativeHookFunctions(functions)
  const inputs = new Set()
  for (const source of sources) {
    if (source.startsWith('/usr/lib/firmware/')) continue
    const interpreters = await inspectSourceElf(source)
    if (!interpreters) continue
    inputs.add(source)
    for (const interpreter of interpreters) {
      const original = await publicSourcePath(interpreter)
      if ((await realpath(join(root, interpreter))) !== join(root, original))
        throw Error(`Copied ELF interpreter differs: ${source}`)
      inputs.add(interpreter)
    }
  }
  const reference = await mkdtemp(join(tmpdir(), '.fde-dependencies-'))
  try {
    await runSource('/bin/sh', [
      '-c',
      `${functions.toString()}\nDESTDIR="$1"\nverbose=n\nshift\nfor source do\ncopy_exec "$source" || exit "$?"\ndone\n`,
      'fde-source-dependencies',
      reference,
      ...inputs,
    ])
    async function requirePaths(path) {
      for (const entry of await readdir(join(reference, path), {
        withFileTypes: true,
      })) {
        const required = join(path, entry.name)
        if (entry.isDirectory()) await requirePaths(required)
        else {
          const expected = relative(
            reference,
            await realpath(join(reference, required)),
          )
          const actual = relative(root, await realpath(join(root, required)))
          if (actual !== expected)
            throw Error(`Copied native dependency differs: ${required}`)
          if (entry.isFile() && !(await lstat(join(root, required))).isFile())
            throw Error(`Copied native dependency is not regular: ${required}`)
        }
      }
    }
    for (const entry of await readdir(reference)) {
      if (entry !== 'usr')
        throw Error(`Native dependency leaves admitted namespace: ${entry}`)
      await requirePaths(entry)
    }
  } finally {
    await rm(reference, { recursive: true, force: true })
  }
}
