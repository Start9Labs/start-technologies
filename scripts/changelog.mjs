#!/usr/bin/env node
import { spawnSync } from 'node:child_process'
import {
  existsSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  unlinkSync,
  writeFileSync,
} from 'node:fs'
import { basename, dirname, resolve } from 'node:path'
import { parseArgs } from 'node:util'
import {
  MANIFESTS,
  TIERS,
  decodeUtf8,
  latestRelease,
  manifestVersion,
  nextVersion,
  parseVersion,
} from './changelog-version.mjs'

const KINDS = ['added', 'changed', 'deprecated', 'removed', 'fixed', 'security']
const FILENAME = new RegExp(
  `^(${TIERS.join('|')})-(${KINDS.join('|')})-[a-z0-9]+(?:-[a-z0-9]+)*\\.md$`,
)
const PRODUCT = /^projects\/([a-z0-9]+(?:-[a-z0-9]+)*)$/

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { maxBuffer: Infinity, ...options })
  if (result.error) throw result.error
  if (result.status !== 0)
    throw new Error(
      decodeUtf8(result.stderr).trim() ||
        `${command} failed (${result.status})`,
    )
  return result.stdout
}

function git(...args) {
  return run('git', args)
}

function productPath(value) {
  value = value.replace(/\/+$/, '')
  if (!PRODUCT.test(value) || !Object.hasOwn(MANIFESTS, basename(value))) {
    throw new Error(`expected projects/<product>: ${JSON.stringify(value)}`)
  }
  return value
}

function parseFragment(path, data) {
  const match = FILENAME.exec(basename(path))
  if (!match || match[0] !== basename(path))
    throw new Error(`${path}: expected <${TIERS.join('|')}>-<kind>-<name>.md`)
  const body = decodeUtf8(data).trim()
  if (!body) throw new Error(`${path}: empty fragment`)
  if (
    /^\s{0,3}#{1,6}(?:\s|$)/m.test(body) ||
    /^\s{0,3}(?:=+|-+)\s*$/m.test(body)
  ) {
    throw new Error(`${path}: headings are not allowed`)
  }
  if (!/^[-+*]\s+\S/.test(body))
    throw new Error(`${path}: body must start with a Markdown bullet item`)
  return { path, tier: match[1], kind: match[2], data, body }
}

function hasSymlink(path) {
  for (let part = resolve(path); ; part = dirname(part)) {
    try {
      if (lstatSync(part).isSymbolicLink()) return true
    } catch (error) {
      if (error.code !== 'ENOENT' && error.code !== 'ENOTDIR') throw error
    }
    if (dirname(part) === part) return false
  }
}

function workingFragments(product) {
  const directory = `${product}/changelog`
  if (hasSymlink(directory))
    throw new Error(`${directory}: symlinks are not allowed`)
  if (!existsSync(directory)) return new Map()
  if (!lstatSync(directory).isDirectory())
    throw new Error(`${directory}: expected directory`)
  const fragments = new Map()
  for (const name of readdirSync(directory).sort()) {
    const path = `${directory}/${name}`
    if (!lstatSync(path).isFile())
      throw new Error(
        `${path}: expected a regular file, not a symlink or directory`,
      )
    fragments.set(path, parseFragment(path, readFileSync(path)))
  }
  return fragments
}

function treeEntry(ref, path) {
  return (
    decodeUtf8(git('ls-tree', '-z', ref, '--', path)).split('\0')[0] || null
  )
}

function regularBlob(metadata, location) {
  const [mode, kind, blob] = metadata.split(' ')
  if (kind !== 'blob' || !['100644', '100755'].includes(mode))
    throw new Error(`${location}: expected a regular file`)
  return git('cat-file', 'blob', blob)
}

function taggedFragments(product, ref) {
  const directory = `${product}/changelog`
  const entry = treeEntry(ref, directory)
  if (entry === null) return new Map()
  if (!entry.startsWith('040000 tree '))
    throw new Error(`${ref}:${directory}: expected directory`)
  const fragments = new Map()
  for (const entry of decodeUtf8(
    git('ls-tree', '-z', `${ref}:${directory}`),
  ).split('\0')) {
    if (!entry) continue
    const separator = entry.indexOf('\t')
    const path = `${directory}/${entry.slice(separator + 1)}`
    fragments.set(
      path,
      parseFragment(
        path,
        regularBlob(entry.slice(0, separator), `${ref}:${path}`),
      ),
    )
  }
  return fragments
}

function taggedHistory(product, ref) {
  const path = `${product}/CHANGELOG.md`
  const entry = treeEntry(ref, path)
  return entry === null
    ? Buffer.alloc(0)
    : regularBlob(entry.split('\t')[0], `${ref}:${path}`)
}

function originTags() {
  const tags = new Map()
  for (const line of decodeUtf8(git('ls-remote', '--tags', 'origin'))
    .trim()
    .split('\n')) {
    if (!line) continue
    let [sha, name] = line.split('\t')
    name = name.replace(/^refs\/tags\//, '')
    const peeled = name.endsWith('^{}')
    name = name.replace(/\^\{\}$/, '')
    const match = /^([a-z0-9]+(?:-[a-z0-9]+)*)\/v(.+)$/.exec(name)
    if (!match || !Object.hasOwn(MANIFESTS, match[1])) continue
    parseVersion(match[1], match[2])
    if (!tags.has(name) || peeled) tags.set(name, sha)
  }
  return tags
}

function compareText(a, b) {
  return a < b ? -1 : a > b ? 1 : 0
}

function originReleases(ref, product = null, tags = originTags()) {
  const ordered = decodeUtf8(git('rev-list', '--topo-order', '--reverse', ref))
    .trim()
    .split('\n')
  const positions = new Map(ordered.map((commit, index) => [commit, index]))
  const releases = []
  for (const [tag, sha] of tags) {
    if (product && !tag.startsWith(`${basename(product)}/v`)) continue
    const commit = decodeUtf8(
      git('rev-parse', '--verify', `${sha}^{commit}`),
    ).trim()
    if (!positions.has(commit)) continue
    const [name, version] = tag.split('/v')
    const path = `projects/${name}`
    releases.push({
      product: path,
      version,
      commit,
      fragments: taggedFragments(path, commit),
    })
  }
  return releases.sort(
    (a, b) =>
      positions.get(a.commit) - positions.get(b.commit) ||
      compareText(a.product, b.product) ||
      compareText(a.version, b.version),
  )
}

function isAncestor(older, newer) {
  const result = spawnSync(
    'git',
    ['merge-base', '--is-ancestor', older, newer],
    { maxBuffer: Infinity },
  )
  if (result.error) throw result.error
  if (![0, 1].includes(result.status))
    throw new Error(decodeUtf8(result.stderr).trim())
  return result.status === 0
}

function deletedBetween(path, source, target) {
  return (
    git(
      'log',
      '--format=%H',
      '--diff-filter=D',
      '--no-renames',
      `${source}..${target}`,
      '--',
      path,
    ).length > 0
  )
}

function ownedFragments(release, earlier) {
  const ancestors = earlier.filter(
    previous =>
      previous.product === release.product &&
      isAncestor(previous.commit, release.commit),
  )
  return [...release.fragments.values()].filter(
    fragment =>
      !ancestors.some(previous => {
        const old = previous.fragments.get(fragment.path)
        return (
          old &&
          old.data.equals(fragment.data) &&
          !deletedBetween(fragment.path, previous.commit, release.commit)
        )
      }),
  )
}

function releaseSection(version, fragments) {
  const lines = [`## [${version}]`, '']
  for (const kind of KINDS) {
    const group = fragments
      .filter(item => item.kind === kind)
      .sort((a, b) => compareText(a.path, b.path))
    if (group.length) {
      lines.push(`### ${kind[0].toUpperCase()}${kind.slice(1)}`, '')
      for (const fragment of group) lines.push(fragment.body, '')
    }
  }
  return Buffer.from(`${lines.join('\n')}\n`)
}

function markdownFormatter() {
  const pin = JSON.parse(decodeUtf8(readFileSync('package.json')))
    .devDependencies.prettier
  const config = resolve('.prettierrc.json')
  const cache = new Map()
  return data => {
    const text = decodeUtf8(data)
    if (!cache.has(text)) {
      cache.set(
        text,
        run(
          'npx',
          [
            '--yes',
            `--package=prettier@${pin}`,
            'prettier',
            '--config',
            config,
            '--parser',
            'markdown',
          ],
          { input: data },
        ),
      )
    }
    return cache.get(text)
  }
}

function prependSection(history, version, fragments, format) {
  if (!fragments.length) return history
  const section = releaseSection(version, fragments)
  const text = decodeUtf8(history)
  const escaped = version.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const heading = new RegExp(
    `^##\\s+\\[${escaped}\\](?:[^\\r\\n]*)\\r?\\n`,
    'm',
  ).exec(text)
  if (heading) {
    const start = heading.index
    const afterHeading = start + heading[0].length
    const next = /^## /m.exec(text.slice(afterHeading))
    const end = next ? afterHeading + next.index : text.length
    if (!format(Buffer.from(text.slice(start, end))).equals(format(section))) {
      throw new Error(
        `release ${version} already has a different compiled changelog; do not move released tags`,
      )
    }
    return history
  }
  const first = /^## /m.exec(text)
  const position = first
    ? Buffer.byteLength(text.slice(0, first.index))
    : history.length
  const prefix = history.subarray(0, position)
  const separator =
    !prefix.length || /(?:\r?\n){2}$/.test(decodeUtf8(prefix))
      ? ''
      : prefix.at(-1) === 10
        ? '\n'
        : '\n\n'
  return Buffer.concat([
    prefix,
    Buffer.from(separator),
    section,
    history.subarray(position),
  ])
}

function compileReleases(history, releases) {
  const original = history
  let formatter
  const format = data => (formatter ??= markdownFormatter())(data)
  const earlier = []
  const consumed = []
  for (const release of releases) {
    const owned = ownedFragments(release, earlier)
    history = prependSection(history, release.version, owned, format)
    consumed.push(...owned.map(fragment => [release.commit, fragment]))
    earlier.push(release)
  }
  return [history.equals(original) ? history : format(history), consumed]
}

function render(product, version, ref) {
  parseVersion(basename(product), version)
  const commit = decodeUtf8(
    git('rev-parse', '--verify', `${ref || 'HEAD'}^{commit}`),
  ).trim()
  const path = `${product}/CHANGELOG.md`
  const history = ref
    ? taggedHistory(product, commit)
    : existsSync(path)
      ? readFileSync(path)
      : Buffer.alloc(0)
  const fragments = ref
    ? taggedFragments(product, commit)
    : workingFragments(product)
  const releases = originReleases(commit, product).filter(
    item => item.version !== version,
  )
  releases.push({ product, version, commit, fragments })
  return compileReleases(history, releases)[0]
}

function checkManifestVersion(
  product,
  version = manifestVersion(process.cwd(), basename(product)),
) {
  const name = basename(product)
  parseVersion(name, version)
  const fragments = workingFragments(product)
  const tags = originTags()
  const baseline = latestRelease(name, tags.keys())
  if (baseline === null) return
  const commit = decodeUtf8(
    git('rev-parse', '--verify', 'HEAD^{commit}'),
  ).trim()
  const releases = originReleases(commit, product, tags)
  const pending = ownedFragments(
    { product, version, commit, fragments },
    releases,
  )
  const expected = nextVersion(
    name,
    baseline,
    pending.map(fragment => fragment.tier),
  )
  if (version !== expected)
    throw new Error(
      `${product}: version '${version}' does not match expected '${expected}' from latest stable origin release '${baseline}' and pending fragments`,
    )
}

function changedProducts(base) {
  const paths = new Set(
    decodeUtf8(
      git('diff', '--name-only', '-z', '--no-renames', `${base}...HEAD`),
    ).split('\0'),
  )
  return Object.entries(MANIFESTS)
    .filter(([name, manifest]) => {
      const directory = `projects/${name}/changelog`
      return (
        paths.has(manifest) ||
        paths.has(directory) ||
        [...paths].some(path => path.startsWith(`${directory}/`))
      )
    })
    .map(([name]) => `projects/${name}`)
}

function sync() {
  const releases = originReleases('HEAD')
  const updates = []
  for (const product of [
    ...new Set(releases.map(release => release.product)),
  ].sort()) {
    const path = `${product}/CHANGELOG.md`
    if (hasSymlink(path)) throw new Error(`${path}: symlinks are not allowed`)
    const history = existsSync(path) ? readFileSync(path) : Buffer.alloc(0)
    const [compiled, consumed] = compileReleases(
      history,
      releases.filter(item => item.product === product),
    )
    updates.push({ path, history, compiled, consumed })
  }
  for (const { path, history, compiled, consumed } of updates) {
    if (!compiled.equals(history)) {
      mkdirSync(dirname(path), { recursive: true })
      writeFileSync(path, compiled)
    }
    for (const [source, fragment] of consumed) {
      if (deletedBetween(fragment.path, source, 'HEAD')) continue
      if (
        hasSymlink(fragment.path) ||
        !existsSync(fragment.path) ||
        !lstatSync(fragment.path).isFile()
      )
        continue
      if (readFileSync(fragment.path).equals(fragment.data))
        unlinkSync(fragment.path)
    }
  }
}

const USAGE =
  'usage: changelog.mjs projects | changed BASE | validate PRODUCT | version PRODUCT | check-version PRODUCT [VERSION] | render PRODUCT VERSION [--ref REF] | sync'

try {
  const { values, positionals } = parseArgs({
    allowPositionals: true,
    options: {
      ref: { type: 'string' },
      help: { type: 'boolean', short: 'h' },
    },
  })
  const [command, ...args] = positionals
  const counts = {
    projects: [0],
    changed: [1],
    validate: [1],
    version: [1],
    'check-version': [1, 2],
    render: [2],
    sync: [0],
  }
  if (
    !values.help &&
    (!counts[command]?.includes(args.length) ||
      (values.ref !== undefined && command !== 'render'))
  ) {
    throw new Error(USAGE)
  }
  if (values.help) console.log(USAGE)
  else if (command === 'projects') console.log(Object.keys(MANIFESTS).join(' '))
  else if (command === 'changed')
    console.log(changedProducts(args[0]).join(' '))
  else if (command === 'validate') workingFragments(productPath(args[0]))
  else if (command === 'version')
    console.log(manifestVersion(process.cwd(), basename(productPath(args[0]))))
  else if (command === 'check-version')
    checkManifestVersion(productPath(args[0]), args[1])
  else if (command === 'render')
    process.stdout.write(render(productPath(args[0]), args[1], values.ref))
  else sync()
} catch (error) {
  process.stderr.write(`changelog: ${error.message}\n`)
  process.exitCode = 1
}
