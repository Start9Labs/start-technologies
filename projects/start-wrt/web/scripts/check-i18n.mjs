import { readFileSync, readdirSync, statSync, existsSync } from 'fs'
import { join, relative, dirname } from 'path'
import { fileURLToPath } from 'url'

const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)
const root = join(__dirname, '..') // web/

const dictDir = join(root, 'src/app/i18n/dictionaries')

// Extract dictionary keys from en.ts (the source of truth)
const enPath = join(dictDir, 'en.ts')
const enSource = readFileSync(enPath, 'utf-8')
const validKeys = new Set()

// Keys may be single- or double-quoted (double is used when the key itself
// contains an apostrophe). Backreference \1 matches the opening quote.
const EN_KEY_RE = /^\s+(['"])(.+?)\1:\s*(\d+)\s*,?\s*$/gm

for (const match of enSource.matchAll(EN_KEY_RE)) {
  validKeys.add(match[2])
}

if (validKeys.size === 0) {
  console.error('ERROR: Could not parse any keys from en.ts')
  process.exit(1)
}

console.log(`Loaded ${validKeys.size} i18n keys from en.ts`)

// Collect all .ts and .html files under src/
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

const files = walk(join(root, 'src'))

// Patterns that mark a string as an i18n key. Scanned against the whole file
// (not line-by-line) so usages prettier wrapped across multiple lines — e.g.
// `{{\n  'long string'\n    | i18n\n}}` — are still caught.
const KEY_PATTERNS = [
  /i18n\.transform\(\s*'([^']+)'\s*,?\s*\)/g, // i18n.transform('Key') (opt. trailing comma)
  /i18n\.transform\(\s*"([^"]+)"\s*,?\s*\)/g, // i18n.transform("Key")
  /'([^']+)'\s*\|\s*i18n\b/g, // 'Key' | i18n
  /"([^"]+)"\s*\|\s*i18n\b/g, // "Key" | i18n
]

const errors = []

for (const file of files) {
  // Skip the dictionary files themselves
  if (file.includes('/i18n/dictionaries/')) continue

  const source = readFileSync(file, 'utf-8')

  for (const pattern of KEY_PATTERNS) {
    for (const m of source.matchAll(pattern)) {
      if (validKeys.has(m[1])) continue
      const line = source.slice(0, m.index).split('\n').length
      errors.push({ file, line, key: m[1] })
    }
  }
}

if (errors.length > 0) {
  console.error(`\nFound ${errors.length} invalid i18n key(s):\n`)
  for (const { file, line, key } of errors) {
    console.error(`  ${relative(root, file)}:${line}  "${key}"`)
  }
  console.error()
}

// Every numeric id in en.ts must exist in every non-English dictionary
const enNumericKeys = new Set()
for (const match of enSource.matchAll(EN_KEY_RE)) {
  enNumericKeys.add(Number(match[3]))
}

const otherLangs = ['de', 'es', 'fr', 'pl']
const dictErrors = []

for (const lang of otherLangs) {
  const dictSource = readFileSync(join(dictDir, `${lang}.ts`), 'utf-8')
  const dictKeys = new Set()

  for (const match of dictSource.matchAll(/^\s*(\d+):/gm)) {
    dictKeys.add(Number(match[1]))
  }

  const missing = [...enNumericKeys]
    .filter(k => !dictKeys.has(k))
    .sort((a, b) => a - b)

  if (missing.length > 0) dictErrors.push({ lang, missing })
}

if (dictErrors.length > 0) {
  console.error(`\nMissing i18n dictionary keys:\n`)
  for (const { lang, missing } of dictErrors) {
    console.error(`  ${lang}.ts is missing keys: ${missing.join(', ')}`)
  }
  console.error()
}

// Help content is split per language (mirroring the i18n dictionaries): English
// is the eager source of truth (content/en.ts) and the universal fallback; each
// other language ships its own lazy file (content/{es,de,fr,pl}.ts). Every route
// in en.ts must exist in all four translation files. Route keys look like
// `  '/route': ` (two-space indent). Tolerant: only runs once the dir exists.
const helpContentDir = join(root, 'src/app/help/content')
const helpLangs = ['es', 'de', 'fr', 'pl']
const helpErrors = []
const ROUTE_RE = /^ {2}('\/[^']+'):/gm
if (existsSync(helpContentDir) && existsSync(join(helpContentDir, 'en.ts'))) {
  const enSrc = readFileSync(join(helpContentDir, 'en.ts'), 'utf-8')
  const enRoutes = [...enSrc.matchAll(ROUTE_RE)].map(m => m[1])

  for (const lang of helpLangs) {
    const path = join(helpContentDir, `${lang}.ts`)
    if (!existsSync(path)) {
      helpErrors.push(`${lang}.ts is missing`)
      continue
    }
    const langRoutes = new Set(
      [...readFileSync(path, 'utf-8').matchAll(ROUTE_RE)].map(m => m[1]),
    )
    const missing = enRoutes.filter(r => !langRoutes.has(r))
    if (missing.length) {
      helpErrors.push(`${lang}.ts is missing routes: ${missing.join(', ')}`)
    }
  }

  if (helpErrors.length > 0) {
    console.error(`\nHelp content missing routes:\n`)
    for (const e of helpErrors) console.error(`  ${e}`)
    console.error()
  }
}

if (errors.length > 0 || dictErrors.length > 0 || helpErrors.length > 0) {
  process.exit(1)
} else {
  console.log('All i18n keys are valid.')
}
