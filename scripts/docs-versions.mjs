#!/usr/bin/env node
import { readFileSync, writeFileSync } from 'node:fs'
import { parseVersion } from './changelog-version.mjs'

// Mirrors the product entries of book_dir() in projects/start-docs/build.sh.
const PRODUCT_BOOKS = Object.freeze({
  'start-os': 'start-os',
  'start-tunnel': 'start-tunnel',
  'start-sdk': 'packaging',
  'start-wrt': 'start-wrt',
})
const LABELLED_BY_RELEASE = new Set(['start-os', 'start-wrt'])

function bookOf(line) {
  return line === '' || line.startsWith('#') ? null : line.split('=')[0]
}

function bookLines(conf) {
  return new Map(
    conf
      .split('\n')
      .filter(line => bookOf(line) !== null)
      .map(line => [bookOf(line), line]),
  )
}

export function releaseLabel(project, version) {
  const [[major, minor]] = parseVersion(project, version)
  return project === 'start-os'
    ? `0.${major}.${minor}.x`
    : `${major}.${minor}.x`
}

// Comments and books outside PRODUCT_BOOKS follow the tagged tree.
export function reconcile(tagged, published, project, version) {
  const own = PRODUCT_BOOKS[project]
  const ownLine = LABELLED_BY_RELEASE.has(project)
    ? `${own}=${releaseLabel(project, version)}`
    : bookLines(tagged).get(own)
  const others = Object.values(PRODUCT_BOOKS).filter(book => book !== own)
  const held = bookLines(published)
  const lines = tagged
    .trimEnd()
    .split('\n')
    .flatMap(line => {
      const book = bookOf(line)
      if (book === own) return [ownLine]
      if (others.includes(book)) return held.has(book) ? [held.get(book)] : []
      return [line]
    })
  const present = new Set(lines.map(bookOf))
  for (const book of [own, ...others]) {
    const line = book === own ? ownLine : held.get(book)
    if (line !== undefined && !present.has(book)) lines.push(line)
  }
  return `${lines.join('\n')}\n`
}

if (process.argv[1]?.endsWith('docs-versions.mjs')) {
  const [project, version, publishedPath, taggedPath] = process.argv.slice(2)
  writeFileSync(
    taggedPath,
    reconcile(
      readFileSync(taggedPath, 'utf8'),
      readFileSync(publishedPath, 'utf8'),
      project,
      version,
    ),
  )
}
