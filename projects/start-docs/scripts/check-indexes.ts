/**
 * Fails when a book's README.md index does not list its SUMMARY.md pages in
 * sidebar order. Usage: node check-indexes.ts
 */

import { globSync, readFileSync } from 'fs'
import { basename, dirname, join } from 'path'

const REPO_DIR = join(import.meta.dirname, '..', '..', '..')

const pageLinks = (file: string) =>
  [...readFileSync(file, 'utf-8').matchAll(/\]\(([^)\s]+)\)/g)]
    .map(m => m[1].split('#')[0].replace(/^\.\//, ''))
    .filter(p => p.endsWith('.md') && !/^[a-z]+:|^\//.test(p))

const summaries = globSync(
  ['projects/*/docs/src/SUMMARY.md', 'projects/start-docs/*/src/SUMMARY.md'],
  { cwd: REPO_DIR },
).sort()

let failed = false
for (const summary of summaries) {
  const src = dirname(summary)
  // The index leaves out the landing page itself and Overview.
  const expected = pageLinks(join(REPO_DIR, summary)).filter(
    p => !['README.md', 'overview.md'].includes(basename(p)),
  )
  // A page's last link is its index entry; prose above the index may link it too.
  const links = pageLinks(join(REPO_DIR, src, 'README.md'))
  const actual = links.filter((p, i) => links.lastIndexOf(p) === i)

  const missing = expected.filter(p => !actual.includes(p))
  const extra = actual.filter(p => !expected.includes(p))
  const at = expected.findIndex((p, i) => p !== actual[i])
  if (!missing.length && !extra.length && at === -1) continue

  failed = true
  console.error(
    `::error file=${src}/README.md::README.md does not match SUMMARY.md`,
  )
  for (const p of missing) console.error(`  ${src}/README.md is missing ${p}`)
  for (const p of extra)
    console.error(`  ${src}/README.md lists ${p}, which SUMMARY.md does not`)
  if (!missing.length && !extra.length)
    console.error(
      `  ${src}/README.md lists ${actual[at]} where SUMMARY.md has ${expected[at]}`,
    )
}

if (failed) process.exit(1)
console.log(`${summaries.length} book indexes match their sidebars.`)
