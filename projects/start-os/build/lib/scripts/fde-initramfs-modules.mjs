import {
  copyFile,
  mkdir,
  mkdtemp,
  realpath,
  rm,
  writeFile,
} from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { admitModuleConfiguration } from './fde-initramfs-control.mjs'
import { filesUnder, kernelDirectory } from './fde-initramfs-paths.mjs'
import {
  publicSourceFile,
  publicSourcePath,
  requireSameBytes,
  requireSameFile,
} from './fde-initramfs-providers.mjs'
import { runSource } from './fde-reviewed-source.mjs'

const depmodInputs = [
  'modules.order',
  'modules.builtin',
  'modules.builtin.modinfo',
]

async function nativeQuery(command, args) {
  const result = await runSource(command, args)
  if (result.stderr.trim())
    throw Error(`Native module query diagnostics: ${result.stderr}`)
  return result.stdout
}

/**
 * Admits source module bytes, native subset indexes and dependencies under effective configuration.
 * Sources/query providers remain authenticated; all inputs remain stable through use.
 * Native tools parse source references, never candidate module bytes; no module is inserted.
 * Firmware and configured alias/request coverage retain separate admission obligations.
 */
export async function admitKernelModules(root) {
  root = await realpath(root)
  const kernel = await kernelDirectory(root)
  if (kernel.version !== (await kernelDirectory('/')).version)
    throw Error('Copied kernel module version differs from source')
  const configuration = await admitModuleConfiguration(root)
  const files = await filesUnder(root, kernel.path)
  const sourceFiles = new Map()
  for (const file of files)
    sourceFiles.set(file, await publicSourceFile(join('/', file)))
  for (const input of depmodInputs) {
    const file = join(kernel.path, input)
    if (!sourceFiles.has(file))
      throw Error(`Missing source module metadata: ${file}`)
  }
  const reference = await mkdtemp(join(tmpdir(), '.fde-modules-'))
  try {
    const input = join(reference, 'input')
    const output = join(reference, 'output')
    for (const [file, source] of sourceFiles) {
      const destination = join(input, file)
      await mkdir(dirname(destination), { recursive: true })
      await copyFile(source, destination)
    }
    await nativeQuery('/usr/sbin/depmod', [
      '--all',
      `--basedir=${input}`,
      `--outdir=${output}`,
      '--moduledir=/usr/lib/modules',
      kernel.version,
    ])
    const generated = new Set(await filesUnder(output, kernel.path))
    for (const file of generated) {
      if (!sourceFiles.has(file))
        throw Error(`Missing generated module metadata: ${file}`)
      await requireSameBytes(join(root, file), join(output, file))
    }
    const admitted = new Set()
    const modules = []
    for (const [file, source] of sourceFiles) {
      if (generated.has(file)) continue
      await requireSameFile(join(root, file), source)
      admitted.add(source)
      let name
      try {
        name = (
          await nativeQuery('/usr/sbin/modinfo', ['--field=name', source])
        ).trim()
      } catch (error) {
        if (error.code !== 1) throw error
        continue
      }
      if (!name || name.includes('\n'))
        throw Error(`Unsupported source module name: ${source}`)
      modules.push(name)
    }
    async function requireDependency(path) {
      const dependency = await publicSourcePath(path)
      if (!admitted.has(dependency))
        throw Error(`Missing native module dependency: ${dependency}`)
    }
    const subjects = join(reference, 'subjects.json')
    await writeFile(subjects, JSON.stringify(modules), { mode: 0o600 })
    const weakDependencies = JSON.parse(
      await nativeQuery('/usr/bin/python3', [
        '-I',
        fileURLToPath(new URL('./fde-module-weakdeps.py', import.meta.url)),
        configuration.library,
        join('/', kernel.path),
        subjects,
        ...configuration.paths,
      ]),
    )
    for (const dependency of weakDependencies)
      await requireDependency(dependency)
    for (const name of modules) {
      // --ignore-install also suppresses soft dependencies.
      const dependencies = await nativeQuery(configuration.executable, [
        '--show-depends',
        ...configuration.paths.flatMap(path => ['--config', path]),
        `--set-version=${kernel.version}`,
        '--',
        name,
      ])
      for (const line of dependencies.trim().split('\n')) {
        if (line.startsWith('builtin ')) continue
        const match = /^insmod (\S+)(?: .*)?$/.exec(line)
        if (!match) throw Error(`Unsupported native module dependency: ${line}`)
        await requireDependency(match[1])
      }
    }
  } finally {
    await rm(reference, { recursive: true, force: true })
  }
}
