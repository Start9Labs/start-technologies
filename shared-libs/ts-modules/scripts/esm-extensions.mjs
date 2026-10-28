// Turns tsc's ESM output into `.mjs` modules native Node loads as written:
// extensioned relative specifiers and `with { type: 'json' }` on JSON imports.
import {
  readdirSync,
  readFileSync,
  renameSync,
  statSync,
  writeFileSync,
} from 'node:fs'
import { dirname, join, resolve } from 'node:path'

const relativeSpecifier =
  /((?:\bfrom|\bimport)\s*\(?\s*)(['"])(\.\.?(?:\/[^'"]*)?)\2/g
const jsonImport = /((?:\bfrom|\bimport)\s*(['"])[^'"]+\.json\2)(?!\s*with\b)/g
const comment = /\/\*[\s\S]*?\*\/|\/\/[^\n]*/g

const isFile = path => statSync(path, { throwIfNoEntry: false })?.isFile()

const resolveRelative = (file, target) => {
  const bare = target.replace(/\/$/, '')
  const base = resolve(dirname(file), bare)
  if (isFile(`${base}.js`)) return `${bare}.mjs`
  if (isFile(join(base, 'index.js'))) return `${bare}/index.mjs`
  throw new Error(`${file}: cannot resolve '${target}'`)
}

const commentRanges = source =>
  [...source.matchAll(comment)].map(m => [m.index, m.index + m[0].length])

const rewrite = file => {
  const source = readFileSync(file, 'utf8')
  const ranges = commentRanges(source)
  const inComment = offset =>
    ranges.some(([start, end]) => start <= offset && offset < end)
  return source
    .replace(relativeSpecifier, (match, lead, quote, target, offset) =>
      /\.m?js$|\.json$/.test(target) || inComment(offset)
        ? match
        : `${lead}${quote}${resolveRelative(file, target)}${quote}`,
    )
    .replace(jsonImport, (match, statement, quote, offset) =>
      inComment(offset) ? match : `${statement} with { type: 'json' }`,
    )
}

const modules = dir =>
  readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
    const path = join(dir, entry.name)
    if (entry.isDirectory()) return modules(path)
    return entry.name.endsWith('.js') ? [path] : []
  })

const files = modules(resolve(process.argv[2]))
const rewritten = files.map(file => [file, rewrite(file)])
for (const [file, source] of rewritten) {
  writeFileSync(file, source)
  renameSync(file, file.replace(/\.js$/, '.mjs'))
}
