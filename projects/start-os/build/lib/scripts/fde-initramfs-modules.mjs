import {
  copyFile,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  readlink,
  realpath,
  rm,
  writeFile,
} from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { admitModuleConfiguration } from './fde-initramfs-control.mjs'
import {
  copyRootAliases,
  filesUnder,
  kernelDirectory,
} from './fde-initramfs-paths.mjs'
import { requireNativeHookFunctions } from './fde-initramfs-producer.mjs'
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

/** Copies admitted source module inputs; rejects native diagnostics even on successful exit. */
export async function collectSourceModuleInputs(destination, version, modules) {
  const functions = await readFile('/usr/share/initramfs-tools/hook-functions')
  requireNativeHookFunctions(functions)
  await nativeQuery('/bin/sh', [
    '-c',
    `${functions.toString()}\nDESTDIR="$1"\nversion="$2"\nverbose=n\nshift 2\n_call_dracut_install "$@"\n`,
    'fde-source-module-closure',
    destination,
    version,
    ...modules,
  ])
}

/**
 * Admits source module bytes, native subset indexes and dependencies under effective configuration.
 * Provider identity admission precedes this call; authenticated sources and all inputs remain stable.
 * Native tools parse source references, never candidate module bytes; no module is inserted.
 * Built-in firmware, configured alias/request coverage and full generator admission remain separate.
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
    const closure = join(reference, 'closure')
    await mkdir(join(closure, 'usr/lib'), { recursive: true })
    await copyRootAliases(root, closure)
    await collectSourceModuleInputs(closure, kernel.version, modules)
    async function requireCollected(path) {
      for (const entry of await readdir(join(closure, path), {
        withFileTypes: true,
      })) {
        const required = join(path, entry.name)
        if (entry.isDirectory()) await requireCollected(required)
        else if (entry.isFile() || entry.isSymbolicLink()) {
          const source = await publicSourcePath(join('/', required))
          const copied = await realpath(join(root, required))
          if (copied !== join(root, source))
            throw Error(`Copied native module input differs: ${required}`)
          if (!admitted.has(source))
            await requireSameFile(copied, await publicSourceFile(source))
        } else throw Error(`Unsupported native module input: ${required}`)
      }
    }
    for (const entry of await readdir(closure, { withFileTypes: true })) {
      if (entry.name === 'usr') await requireCollected('usr')
      else if (
        !entry.isSymbolicLink() ||
        (await readlink(join(closure, entry.name))) !==
          (await readlink(join(root, entry.name)))
      )
        throw Error(
          `Native module input leaves admitted namespace: ${entry.name}`,
        )
    }
  } finally {
    await rm(reference, { recursive: true, force: true })
  }
}
