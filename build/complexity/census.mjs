#!/usr/bin/env node
import { spawnSync } from 'node:child_process'
import { mkdtempSync, readFileSync, readdirSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, relative, resolve, sep } from 'node:path'
import { parseArgs } from 'node:util'

const SCOPES = ['shared-libs', 'projects']
const EXCLUDED_DIRECTORIES = new Set([
  'node_modules',
  'dist',
  'target',
  '.angular',
  'out-tsc',
  'osBindings',
  'locales',
  '__snapshots__',
  '__fixtures__',
])
const EXCLUDED_PATHS = ['/patch-db/client/', '/exver/exver.ts']

function subsystem(path) {
  const parts = path.split('/')
  const size =
    parts[0] === 'shared-libs' && parts.length > 3
      ? 3
      : parts[0] === 'projects' && parts.length > 2
        ? 2
        : 1
  const rest = parts.slice(size).filter(p => p !== 'src' && p !== 'lib')
  return (
    parts.slice(0, size).join('/') +
    (rest.length && !rest[0].includes('.') ? '/' + rest[0] : '')
  )
}

function kept(path) {
  return (
    /\.(rs|ts|js)$/.test(path) &&
    !/\.(spec|d)\.ts$/.test(path) &&
    !path.split('/').some(part => EXCLUDED_DIRECTORIES.has(part)) &&
    !EXCLUDED_PATHS.some(part => ('/' + path).includes(part))
  )
}

function* files(dir, prune = new Set()) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name)
    if (entry.isDirectory()) {
      if (!prune.has(entry.name)) yield* files(path, prune)
    } else yield path
  }
}

function* spaces(node, root = true) {
  if (node.kind === 'function' && !root) yield node
  for (const child of node.spaces ?? []) yield* spaces(child, false)
}

function analyze(bca, args) {
  const result = spawnSync(bca, args, {
    encoding: 'utf8',
    maxBuffer: 32 * 1024 * 1024,
  })
  if (result.error || result.status !== 0) {
    const reason =
      result.error?.message ||
      result.stderr.trim().split('\n').at(-1) ||
      result.signal ||
      'no output'
    throw new Error(
      `complexity: ${bca} ${args[0]} failed (${result.status}): ${reason}`,
    )
  }
  return result.stdout
}

function census(root, scopes, bca) {
  const out = mkdtempSync(join(tmpdir(), 'cx-'))
  const rows = []
  try {
    analyze(bca, [
      'metrics',
      '-O',
      'json',
      '--exclude-tests',
      '--output-dir',
      out,
      ...scopes.map(s => join(root, s)),
    ])
    for (const path of files(out)) {
      if (!path.endsWith('.json')) continue
      const doc = JSON.parse(readFileSync(path, 'utf8'))
      const rel = relative(root, doc.name ?? '')
        .split(sep)
        .join('/')
      if (!kept(rel)) continue
      for (const fn of spaces(doc)) {
        const m = fn.metrics ?? {}
        const label = fn.name ?? ''
        rows.push({
          file: rel,
          name: !label || label.includes(sep) ? '<anonymous>' : label,
          line: fn.start_line,
          cognitive: Math.trunc(m.cognitive?.value || 0),
          cyclomatic: Math.trunc(m.cyclomatic?.value || 0),
          sloc: Math.trunc(m.loc?.sloc || 0),
        })
      }
    }
  } finally {
    rmSync(out, { recursive: true, force: true })
  }
  return rows
}

function countCallers(rows, root, scopes) {
  const perFile = new Map()
  const counts = new Map()
  const definitions = new Map()
  const callersOf = new Map()
  for (const scope of scopes) {
    for (const path of files(join(root, scope), EXCLUDED_DIRECTORIES)) {
      const rel = relative(root, path).split(sep).join('/')
      if (!kept(rel)) continue
      const local = new Map()
      for (const [, name] of readFileSync(path, 'utf8').matchAll(
        /(?<![\p{L}\p{N}_])([A-Za-z_][\p{L}\p{N}_]*)\s*[(:<]/gu,
      )) {
        local.set(name, (local.get(name) ?? 0) + 1)
      }
      perFile.set(rel, local)
    }
  }
  for (const [file, local] of perFile) {
    for (const [name, count] of local) {
      counts.set(name, (counts.get(name) ?? 0) + count)
      if (!callersOf.has(name)) callersOf.set(name, new Set())
      callersOf.get(name).add(subsystem(file))
    }
  }
  for (const row of rows)
    definitions.set(row.name, (definitions.get(row.name) ?? 0) + 1)
  for (const row of rows) {
    if (row.name === '<anonymous>') continue
    const total = counts.get(row.name) ?? 0
    row.callers = Math.max(0, total - definitions.get(row.name))
    row.shared = total - (perFile.get(row.file)?.get(row.name) ?? 0) > 0
    row.util = /(^|\/)(util|utils|helpers)(\/|\.(rs|ts|js)$)/.test(row.file)
    row.scopes = [...(callersOf.get(row.name) ?? [])]
      .filter(s => s !== subsystem(row.file))
      .sort()
  }
}

function assertParsed(bca, root, scopes, rows) {
  if (!rows.length)
    throw new Error(
      'complexity: the census found no functions at all — the analyzer did not run',
    )
  const out = analyze(bca, [
    'count',
    '--type',
    'ERROR',
    ...scopes.map(s => join(root, s)),
  ])
  const value = label =>
    Number(
      out
        .match(new RegExp(`^${label}[^\\n]*:\\s*([\\d,]+)`, 'm'))?.[1]
        .replaceAll(',', '') ?? 0,
    )
  const total = value('Total nodes')
  const found = value('Found nodes')
  if (!total) throw new Error('complexity: the analyzer reported no AST nodes')
  if (found / total > 0.005) {
    throw new Error(
      `complexity: ${((found / total) * 100).toFixed(3)}% of AST nodes are parse errors — the grammar has fallen behind the language`,
    )
  }
}

function totals(rows) {
  return {
    functions: rows.length,
    cognitive: rows.reduce((n, row) => n + row.cognitive, 0),
    cyclomatic: rows.reduce((n, row) => n + row.cyclomatic, 0),
    sloc: rows.reduce((n, row) => n + row.sloc, 0),
    over25: rows.filter(row => row.cognitive > 25).length,
  }
}

function main() {
  const { values } = parseArgs({
    options: {
      root: { type: 'string', default: '.' },
      bca: { type: 'string', default: process.env.BCA || 'bca' },
      scope: { type: 'string', multiple: true },
      json: { type: 'boolean' },
      totals: { type: 'boolean' },
      top: { type: 'string', default: '25' },
    },
  })
  const root = resolve(values.root)
  const scopes = values.scope ?? SCOPES
  const rows = census(root, scopes, values.bca)
  assertParsed(values.bca, root, scopes, rows)
  const t = totals(rows)
  if (values.json) {
    countCallers(rows, root, scopes)
    process.stdout.write(JSON.stringify({ totals: t, functions: rows }))
  } else if (values.totals) {
    console.log(Object.values(t).join('\t'))
  } else {
    const top = Number(values.top)
    if (!Number.isInteger(top))
      throw new Error('complexity: --top must be an integer')
    console.log(
      `functions ${t.functions}  cognitive ${t.cognitive}  cyclomatic ${t.cyclomatic}  sloc ${t.sloc}  over25 ${t.over25}`,
    )
    for (const row of rows
      .sort((a, b) => b.cognitive - a.cognitive)
      .slice(0, top)) {
      console.log(
        `  ${String(row.cognitive).padStart(5)} ${String(row.sloc).padStart(5)}  ${row.name.slice(0, 34).padEnd(34)} ${row.file}:${row.line}`,
      )
    }
  }
}

try {
  main()
} catch (error) {
  console.error(error.message)
  process.exitCode = 1
}
