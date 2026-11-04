import { globSync, readFileSync } from 'fs'
import { basename, dirname, join } from 'path'

const REPO_DIR = join(import.meta.dirname, '..', '..', '..')

const indexLinks = (file: string) =>
  [
    ...readFileSync(file, 'utf-8').matchAll(
      /^\s*[-*+]\s+(?:\*\*)?\[[^\]\n]+\]\(([^)\s]+)\)/gm,
    ),
  ]
    .map(m => m[1].split('#')[0].replace(/^\.\//, ''))
    .filter(p => p.endsWith('.md') && !/^[a-z]+:|^\//.test(p))

const summaries = globSync(
  ['projects/*/docs/src/SUMMARY.md', 'projects/start-docs/*/src/SUMMARY.md'],
  { cwd: REPO_DIR },
).sort()

let failed = false
for (const summary of summaries) {
  const src = dirname(summary)
  const expected = indexLinks(join(REPO_DIR, summary)).filter(
    p => !['README.md', 'overview.md'].includes(basename(p)),
  )
  const actual = indexLinks(join(REPO_DIR, src, 'README.md'))

  const missing = expected.filter(p => !actual.includes(p))
  const extra = actual.filter(p => !expected.includes(p))
  const at = expected.findIndex((p, i) => p !== actual[i])
  if (at === -1 && actual.length === expected.length) continue

  failed = true
  console.error(
    `::error file=${src}/README.md::README.md does not match SUMMARY.md`,
  )
  for (const p of missing) console.error(`  ${src}/README.md is missing ${p}`)
  for (const p of extra)
    console.error(`  ${src}/README.md lists ${p}, which SUMMARY.md does not`)
  if (!missing.length && !extra.length) {
    const position = at === -1 ? expected.length : at
    console.error(
      `  ${src}/README.md lists ${actual[position] ?? '(end)'} where SUMMARY.md has ${expected[position] ?? '(end)'}`,
    )
  }
}

if (failed) process.exit(1)
console.log(`${summaries.length} book indexes match their sidebars.`)
