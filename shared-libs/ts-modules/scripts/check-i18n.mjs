import { readFileSync, readdirSync, statSync } from 'fs'
import { dirname, join, relative } from 'path'
import { fileURLToPath } from 'url'
import { dictionaryEntries } from './i18n-dictionary.mjs'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const english = dictionaryEntries(
  readFileSync(join(root, 'shared/src/i18n/dictionaries/en.ts'), 'utf-8'),
)
const validKeys = new Set(english.keys())

if (validKeys.size === 0) {
  console.error('ERROR: Could not parse any keys from en.ts')
  process.exit(1)
}
console.log(`Loaded ${validKeys.size} i18n keys from en.ts`)

function walk(dir, files = []) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry)
    if (entry === 'node_modules' || entry === 'dist') continue
    if (statSync(full).isDirectory()) walk(full, files)
    else if (full.endsWith('.ts') || full.endsWith('.html')) files.push(full)
  }
  return files
}

const scanDirs = [
  'shared',
  'marketplace',
  '../../projects/start-os/web/ui',
  '../../projects/start-os/web/setup-wizard',
  '../../projects/brochure-marketplace',
].map(d => join(root, d))
const errors = []
for (const file of scanDirs.flatMap(d => walk(d))) {
  if (file.includes('/i18n/dictionaries/')) continue
  const lines = readFileSync(file, 'utf-8').split('\n')
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]
    for (const pattern of [
      /i18n\.transform\(\s*'([^']+)'\s*\)/g,
      /i18n\.transform\(\s*"([^"]+)"\s*\)/g,
      /'([^']+)'\s*\|\s*i18n/g,
      /"([^"]+)"\s*\|\s*i18n/g,
    ]) {
      for (const match of line.matchAll(pattern)) {
        if (!validKeys.has(match[1]))
          errors.push({ file, line: i + 1, key: match[1] })
      }
    }
  }
}
if (errors.length) {
  console.error(`\nFound ${errors.length} invalid i18n key(s):\n`)
  for (const { file, line, key } of errors)
    console.error(`  ${relative(root, file)}:${line}  "${key}"`)
  console.error()
}

const numericKeys = new Set(english.values())
const dictErrors = []
for (const lang of ['de', 'es', 'fr', 'pl']) {
  const entries = dictionaryEntries(
    readFileSync(
      join(root, 'shared/src/i18n/dictionaries', `${lang}.ts`),
      'utf-8',
    ),
  )
  const ids = new Set([...entries.keys()].map(Number))
  const missing = [...numericKeys]
    .filter(id => !ids.has(id))
    .sort((a, b) => a - b)
  if (missing.length) dictErrors.push({ lang, missing })
}
if (dictErrors.length) {
  console.error('\nMissing i18n dictionary keys:\n')
  for (const { lang, missing } of dictErrors)
    console.error(`  ${lang}.ts is missing keys: ${missing.join(', ')}`)
  console.error()
}
if (errors.length || dictErrors.length) process.exit(1)
console.log('All i18n keys are valid.')
