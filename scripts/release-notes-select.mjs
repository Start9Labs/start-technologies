#!/usr/bin/env node
import { spawnSync } from 'node:child_process'
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { emitKeypressEvents } from 'node:readline'

const USAGE =
  'usage: release-notes-select.mjs CANDIDATES SELECTION [TITLE]\n' +
  'Reads changelog.mjs candidates JSON and writes the chosen { kind, body } items.'

const [input, output, title = 'release notes'] = process.argv.slice(2)
if (!input || !output) {
  process.stderr.write(`${USAGE}\n`)
  process.exit(2)
}

const items = JSON.parse(readFileSync(input, 'utf8')).map(item => ({
  ...item,
  reworded: false,
}))
let cursor = 0
let top = 0

function save() {
  writeFileSync(
    output,
    `${JSON.stringify(
      items
        .filter(item => item.selected)
        .map(({ path, kind, body }) => ({ path, kind, body })),
      null,
      2,
    )}\n`,
  )
}

if (!items.length) {
  save()
  process.exit(0)
}

const out = process.stdout
const input$ = process.stdin

function summary(body) {
  return body
    .replace(/^[-+*]\s+/, '')
    .replace(/\s+/g, ' ')
    .trim()
}

function draw() {
  const width = out.columns || 100
  const height = Math.max((out.rows || 24) - 4, 3)
  if (cursor < top) top = cursor
  if (cursor >= top + height) top = cursor - height + 1
  const lines = [
    `Select the fragments for the ${title}`,
    '↑/↓ move · space toggle · tab reword · enter confirm · q abort',
    '',
  ]
  for (const [index, item] of items.slice(top, top + height).entries()) {
    const position = top + index
    const line = `${position === cursor ? '>' : ' '} [${item.selected ? 'x' : ' '}] ${item.kind.padEnd(10)}${item.reworded ? '*' : ' '}${summary(item.body)}`
    lines.push(line.length > width ? `${line.slice(0, width - 1)}…` : line)
  }
  out.write(`\x1b[H\x1b[2J${lines.join('\n')}\n`)
}

function setRaw(raw) {
  if (input$.isTTY) input$.setRawMode(raw)
}

function reword(item) {
  const directory = mkdtempSync(join(tmpdir(), 'release-notes-'))
  const file = join(directory, 'fragment.md')
  try {
    writeFileSync(file, `${item.body.trim()}\n`)
    const editor = (process.env.VISUAL || process.env.EDITOR || 'vi').split(
      /\s+/,
    )
    setRaw(false)
    input$.pause()
    out.write('\x1b[H\x1b[2J')
    const result = spawnSync(editor[0], [...editor.slice(1), file], {
      stdio: 'inherit',
    })
    if (result.status === 0) {
      const body = readFileSync(file, 'utf8').trim()
      if (body && body !== item.body.trim()) {
        item.body = body
        item.reworded = true
        item.selected = true
      }
    }
  } finally {
    rmSync(directory, { recursive: true, force: true })
    input$.resume()
    setRaw(true)
  }
}

function finish(code) {
  setRaw(false)
  input$.pause()
  out.write('\x1b[H\x1b[2J')
  if (code === 0) save()
  process.exit(code)
}

emitKeypressEvents(input$)
setRaw(true)
input$.on('keypress', (text, key = {}) => {
  if (
    (key.ctrl && key.name === 'c') ||
    key.name === 'q' ||
    key.name === 'escape'
  )
    return finish(1)
  if (key.name === 'up' || key.name === 'k')
    cursor = (cursor + items.length - 1) % items.length
  else if (key.name === 'down' || key.name === 'j')
    cursor = (cursor + 1) % items.length
  else if (key.name === 'space' || text === ' ')
    items[cursor].selected = !items[cursor].selected
  else if (key.name === 'tab') reword(items[cursor])
  else if (key.name === 'return' || key.name === 'enter') return finish(0)
  draw()
})
input$.on('end', () => finish(1))
draw()
