import { lstat, readdir } from 'node:fs/promises'
import { join } from 'node:path'

export async function checkedDirectory(root, relative) {
  let parent = root
  for (const part of relative.split('/')) {
    parent = join(parent, part)
    if (!(await lstat(parent)).isDirectory())
      throw Error(`Unsupported generated directory: ${relative}`)
  }
  return parent
}

export async function filesUnder(root, relative) {
  const entries = []
  for (const entry of await readdir(await checkedDirectory(root, relative), {
    withFileTypes: true,
  })) {
    const path = `${relative}/${entry.name}`
    if (entry.isDirectory()) entries.push(...(await filesUnder(root, path)))
    else if (entry.isFile()) entries.push(path)
    else throw Error(`Unsupported generated file type: ${path}`)
  }
  return entries
}

export async function kernelDirectory(root) {
  const directory = await checkedDirectory(root, 'usr/lib/modules')
  const versions = await readdir(directory, { withFileTypes: true })
  if (versions.length !== 1 || !versions[0].isDirectory())
    throw Error('Expected one generated kernel module directory')
  const version = versions[0].name
  return { version, path: `usr/lib/modules/${version}` }
}
