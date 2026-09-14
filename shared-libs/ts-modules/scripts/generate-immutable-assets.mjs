import { randomUUID } from 'node:crypto'
import { lstat, mkdir, readFile, rename, rm, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const manifestName = 'immutable-assets.txt'

function validateOutputPath(outputPath, browserRoot) {
  if (
    outputPath.includes('\\') ||
    outputPath.includes('\n') ||
    outputPath.includes('\r') ||
    outputPath.includes('\0') ||
    path.posix.isAbsolute(outputPath) ||
    path.win32.isAbsolute(outputPath) ||
    path.posix.normalize(outputPath) !== outputPath
  ) {
    throw new Error(`Unsafe output path: ${JSON.stringify(outputPath)}`)
  }

  const root = path.resolve(browserRoot)
  const resolved = path.resolve(root, outputPath)
  if (resolved === root || !resolved.startsWith(`${root}${path.sep}`)) {
    throw new Error(
      `Output path escapes browser root: ${JSON.stringify(outputPath)}`,
    )
  }

  return resolved
}

export async function collectImmutableAssets(outputPaths, browserRoot) {
  const assets = new Set()

  for (const outputPath of outputPaths) {
    const resolved = validateOutputPath(outputPath, browserRoot)
    try {
      if ((await lstat(resolved)).isFile()) assets.add(outputPath)
    } catch (error) {
      if (error.code !== 'ENOENT') throw error
    }
  }

  return [...assets].sort()
}

async function writeManifest(browserRoot, assets) {
  await mkdir(browserRoot, { recursive: true })
  const manifestPath = path.join(browserRoot, manifestName)
  const temporaryPath = `${manifestPath}.${process.pid}.${randomUUID()}.tmp`

  try {
    await writeFile(temporaryPath, `${assets.join('\n')}\n`, {
      encoding: 'utf8',
      flag: 'wx',
    })
    await rename(temporaryPath, manifestPath)
  } catch (error) {
    await rm(temporaryPath, { force: true })
    throw error
  }

  return manifestPath
}

export async function generateImmutableAssets(statsPath, browserRoot) {
  const stats = JSON.parse(await readFile(statsPath, 'utf8'))
  if (
    !stats.outputs ||
    typeof stats.outputs !== 'object' ||
    Array.isArray(stats.outputs)
  ) {
    throw new Error('Angular stats must contain an outputs object')
  }

  const assets = await collectImmutableAssets(
    Object.keys(stats.outputs),
    browserRoot,
  )
  if (assets.length === 0) {
    throw new Error('Angular stats contain no physical generated outputs')
  }

  const manifestPath = await writeManifest(browserRoot, assets)
  await rm(statsPath)
  return manifestPath
}

export function writeEmptyImmutableAssets(browserRoot) {
  return writeManifest(browserRoot, [])
}

async function main() {
  const [modeOrStats, browserRoot, extra] = process.argv.slice(2)
  if (!modeOrStats || !browserRoot || extra) {
    throw new Error(
      'Usage: generate-immutable-assets.mjs <stats.json> <browser-root> | --empty <browser-root>',
    )
  }

  if (modeOrStats === '--empty') {
    await writeEmptyImmutableAssets(browserRoot)
  } else {
    await generateImmutableAssets(modeOrStats, browserRoot)
  }
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  await main()
}
