import { readFile, rm, stat, writeFile } from 'node:fs/promises'
import path from 'node:path'

const [statsPath, browserRoot] = process.argv.slice(2)
const assets = []

if (statsPath !== '--empty') {
  const { outputs } = JSON.parse(await readFile(statsPath, 'utf8'))
  for (const output of Object.keys(outputs).sort()) {
    const isFile = await stat(path.join(browserRoot, output)).then(
      s => s.isFile(),
      () => false,
    )
    if (isFile) assets.push(output)
  }
  if (assets.length === 0) {
    throw new Error(`${statsPath} lists no output file under ${browserRoot}`)
  }
  await rm(statsPath)
}

await writeFile(
  path.join(browserRoot, 'immutable-assets.txt'),
  assets.map(asset => `${asset}\n`).join(''),
)
