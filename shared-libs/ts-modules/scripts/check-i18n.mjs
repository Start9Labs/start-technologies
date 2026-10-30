import { readFileSync, readdirSync, statSync } from 'fs'
import { join, relative } from 'path'
import { fileURLToPath } from 'url'
import { dirname } from 'path'

const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)
const root = join(__dirname, '..')

const enPath = join(root, 'shared/src/i18n/dictionaries/en.ts')
const enSource = readFileSync(enPath, 'utf-8')
const englishEntries = [...enSource.matchAll(/^\s+(['"])(.+?)\1:\s*(\d+)/gm)]
const validKeys = new Set(englishEntries.map(match => match[2]))

if (validKeys.size === 0) {
  console.error('ERROR: Could not parse any keys from en.ts')
  process.exit(1)
}

console.log(`Loaded ${validKeys.size} i18n keys from en.ts`)

function walk(dir, files = []) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry)
    if (entry === 'node_modules' || entry === 'dist') continue
    const stat = statSync(full)
    if (stat.isDirectory()) {
      walk(full, files)
    } else if (full.endsWith('.ts') || full.endsWith('.html')) {
      files.push(full)
    }
  }
  return files
}

// StartTunnel and StartWRT use their own dictionaries and validation.
const scanDirs = [
  'shared',
  'marketplace',
  '../../projects/start-os/web/ui',
  '../../projects/start-os/web/setup-wizard',
  '../../projects/brochure-marketplace',
].map(d => join(root, d))
const files = scanDirs.flatMap(d => walk(d))

const errors = []

for (const file of files) {
  if (file.includes('/i18n/dictionaries/')) continue

  const source = readFileSync(file, 'utf-8')
  const lines = source.split('\n')

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]

    for (const m of line.matchAll(/i18n\.transform\(\s*'([^']+)'\s*\)/g)) {
      if (!validKeys.has(m[1])) {
        errors.push({ file, line: i + 1, key: m[1] })
      }
    }
    for (const m of line.matchAll(/i18n\.transform\(\s*"([^"]+)"\s*\)/g)) {
      if (!validKeys.has(m[1])) {
        errors.push({ file, line: i + 1, key: m[1] })
      }
    }

    for (const m of line.matchAll(/'([^']+)'\s*\|\s*i18n/g)) {
      if (!validKeys.has(m[1])) {
        errors.push({ file, line: i + 1, key: m[1] })
      }
    }
    for (const m of line.matchAll(/"([^"]+)"\s*\|\s*i18n/g)) {
      if (!validKeys.has(m[1])) {
        errors.push({ file, line: i + 1, key: m[1] })
      }
    }
  }
}

if (errors.length > 0) {
  console.error(`\nFound ${errors.length} invalid i18n key(s):\n`)
  for (const { file, line, key } of errors) {
    const rel = relative(root, file)
    console.error(`  ${rel}:${line}  "${key}"`)
  }
  console.error()
}

const enNumericKeys = new Set(englishEntries.map(match => Number(match[3])))

const dictDir = join(root, 'shared/src/i18n/dictionaries')
const otherLangs = ['de', 'es', 'fr', 'pl']
const dictErrors = []

for (const lang of otherLangs) {
  const dictPath = join(dictDir, `${lang}.ts`)
  const dictSource = readFileSync(dictPath, 'utf-8')
  const dictKeys = new Set()

  for (const match of dictSource.matchAll(/^\s*(\d+):/gm)) {
    dictKeys.add(Number(match[1]))
  }

  const missing = [...enNumericKeys]
    .filter(k => !dictKeys.has(k))
    .sort((a, b) => a - b)

  if (missing.length > 0) {
    dictErrors.push({ lang, missing })
  }
}

if (dictErrors.length > 0) {
  console.error(`\nMissing i18n dictionary keys:\n`)
  for (const { lang, missing } of dictErrors) {
    console.error(`  ${lang}.ts is missing keys: ${missing.join(', ')}`)
  }
  console.error()
}

if (errors.length > 0 || dictErrors.length > 0) {
  process.exit(1)
} else {
  console.log('All i18n keys are valid.')
}
