import { lstat, readdir, readlink, realpath } from 'node:fs/promises'
import {
  basename,
  dirname,
  isAbsolute,
  join,
  relative,
  resolve,
} from 'node:path'
import { runSource } from './fde-reviewed-source.mjs'

function contained(parent, path) {
  const suffix = relative(parent, path)
  return suffix === '' || (!suffix.startsWith('../') && suffix !== '..')
}

/** Resolves a public path within the admitted source namespace. */
export async function publicSourcePath(path) {
  const source = await realpath(path)
  if (!contained('/usr', source))
    throw Error(`Unsupported source provider path: ${path}`)
  return source
}

/**
 * Returns canonical source paths after copied /usr byte/mode and native command/root alias admission.
 * The source namespace and private hooks must remain admitted and unchanged throughout the call.
 * Provider links are canonical and relative; admission precedes private control-plane rewriting.
 * Kernel modules, generated control/configuration and complete loader/EARLY closure have separate owners.
 */
export async function admitCopiedProviders(root, readNativeAliases) {
  root = await realpath(root)
  const links = []
  const sources = []
  async function walk(path) {
    if (!(await lstat(join(root, path))).isDirectory())
      throw Error(`Unsupported provider directory: ${path}`)
    if (path === 'usr/lib/modules') return
    for (const entry of await readdir(join(root, path), {
      withFileTypes: true,
    })) {
      const child = join(path, entry.name)
      if (entry.isDirectory()) await walk(child)
      else if (entry.isSymbolicLink()) links.push(child)
      else if (entry.isFile()) {
        const source = join('/', child)
        if (
          !(await lstat(source)).isFile() ||
          (await publicSourcePath(source)) !== source
        )
          throw Error(`Unsupported regular provider path: ${child}`)
        const actual = await lstat(join(root, child))
        const expected = await lstat(source)
        if ((actual.mode & 0o7777) !== (expected.mode & 0o7777))
          throw Error(`Copied provider permissions differ: ${child}`)
        await runSource('/usr/bin/cmp', [
          '--silent',
          '--',
          join(root, child),
          source,
        ])
        sources.push(source)
      } else throw Error(`Unsupported provider file type: ${child}`)
    }
  }
  await walk('usr')
  for (const entry of await readdir(root, { withFileTypes: true }))
    if (entry.isSymbolicLink()) links.push(entry.name)
  const aliases = new Map()
  for (const [alias, provider] of await readNativeAliases()) {
    const absolute = resolve('/', alias)
    const parent = await publicSourcePath(dirname(absolute))
    const path = join(parent, basename(absolute)).slice(1)
    const target = (await publicSourcePath(provider)).slice(1)
    if (!aliases.has(path)) aliases.set(path, new Set())
    aliases.get(path).add(target)
  }
  for (const path of links) {
    const link = await readlink(join(root, path))
    if (isAbsolute(link))
      throw Error(`Copied provider links must be relative: ${path}`)
    const target = await realpath(join(root, path))
    if (!contained(join(root, 'usr'), target))
      throw Error(`Copied provider link escapes public tree: ${path}`)
    if (link !== relative(dirname(join(root, path)), target))
      throw Error(`Copied provider link is not canonical relative: ${path}`)
    const destination = relative(root, target)
    if (aliases.get(path)?.has(destination)) continue
    let source
    try {
      source = await publicSourcePath(join('/', path))
    } catch (error) {
      if (error.code !== 'ENOENT') throw error
    }
    if (source !== join('/', destination))
      throw Error(`Unsupported copied provider alias: ${path}`)
  }
  return sources
}
