#!/usr/bin/env node
import { readFileSync } from 'node:fs'

const [basePath, headPath, ref] = process.argv.slice(2)
const base = JSON.parse(readFileSync(basePath, 'utf8'))
const head = JSON.parse(readFileSync(headPath, 'utf8'))
const key = row =>
  JSON.stringify([
    row.file,
    row.name,
    row.name === '<anonymous>' ? row.sloc : 0,
  ])
const B = new Map(base.functions.map(row => [key(row), row]))
const H = new Map(head.functions.map(row => [key(row), row]))
const sum = (rows, field) => rows.reduce((n, row) => n + row[field], 0)
const added = [...H]
  .filter(([k]) => !B.has(k))
  .map(([, row]) => row)
  .sort((a, b) => b.cognitive - a.cognitive)

console.log(`Complexity vs ${ref.slice(0, 10)}`)
for (const [label, field] of [
  ['functions', 'functions'],
  ['cognitive', 'cognitive'],
  ['cyclomatic', 'cyclomatic'],
  ['sloc', 'sloc'],
  ['fns over 25', 'over25'],
]) {
  const delta = head.totals[field] - base.totals[field]
  console.log(
    `  ${label.padEnd(12)}${String(base.totals[field]).padStart(7)} -> ${String(head.totals[field]).padStart(7)}   ${delta >= 0 ? '+' : ''}${delta}`,
  )
}

const big = added.filter(row => row.cognitive > 10)
if (big.length) {
  console.log(
    `\n  new functions over cognitive 10 (${big.length} of ${added.length} new):`,
  )
  for (const row of big.slice(0, 10)) {
    console.log(
      `    cog ${String(row.cognitive).padStart(4)}  ${row.name}  ${row.file}:${row.line}`,
    )
  }
}

const changed = [...H]
  .filter(([k]) => B.has(k))
  .map(([k, row]) => ({ row, old: B.get(k).cognitive }))
const worse = changed
  .filter(({ row, old }) => row.cognitive > old)
  .sort((a, b) => b.row.cognitive - b.old - (a.row.cognitive - a.old))
if (worse.length) {
  console.log(`\n  existing functions made more complex (${worse.length}):`)
  for (const { row, old } of worse.slice(0, 10)) {
    const flag =
      old > 25
        ? '  <-- already over 25'
        : row.cognitive > 25
          ? '  <-- now over 25'
          : ''
    console.log(
      `    cog ${old} -> ${row.cognitive}  ${row.name}  ${row.file}:${row.line}${flag}`,
    )
  }
}

const better = changed
  .filter(({ row, old }) => row.cognitive < old)
  .sort((a, b) => a.row.cognitive - a.old - (b.row.cognitive - b.old))
if (better.length) {
  console.log(`\n  simplified (${better.length}):`)
  for (const { row, old } of better.slice(0, 5)) {
    console.log(
      `    cog ${old} -> ${row.cognitive}  ${row.name}  ${row.file}:${row.line}`,
    )
  }
}

const gone = [...B].filter(([k]) => !H.has(k)).map(([, row]) => row)
if (gone.length)
  console.log(
    `\n  removed: ${gone.length} functions, ${sum(gone, 'cognitive')} cognitive`,
  )

const adopted = []
for (const [k, row] of H) {
  const before = new Set(B.get(k)?.scopes ?? [])
  const after = new Set(row.scopes ?? [])
  const gained = [...after].filter(scope => !before.has(scope)).sort()
  if (gained.length && after.size >= 2)
    adopted.push({ row, gained, total: after.size })
}
if (adopted.length) {
  console.log(
    `\n  utilities a second subsystem now depends on (${adopted.length}):`,
  )
  for (const { row, gained, total } of adopted
    .sort((a, b) => b.gained.length - a.gained.length)
    .slice(0, 8)) {
    console.log(
      `    ${row.name}  +${gained.join(', ')}  (now ${total})  ${row.file}`,
    )
  }
}

const utilNew = added.filter(row => row.util)
if (utilNew.length)
  console.log(
    `  of the new functions, ${utilNew.length} sit in util modules (${sum(utilNew, 'cognitive')} cognitive)`,
  )

const privateHelpers = added.filter(
  row =>
    row.callers === 1 && !row.shared && !row.util && row.name !== '<anonymous>',
)
if (privateHelpers.length)
  console.log(
    `  single-use helpers alongside their only caller: ${privateHelpers.length}`,
  )
