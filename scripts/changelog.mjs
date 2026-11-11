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
const LOCALE_PATTERN = '[a-z]{2}_[A-Z]{2}'
const LOCALE = new RegExp(`^${LOCALE_PATTERN}$`)
const TRANSLATION = new RegExp(`^(.+)\\.(${LOCALE_PATTERN})\\.md$`)
const ENGLISH = 'en_US'
const NOTES_ASSET = 'release-notes.json'
const PRE_UPDATE_START = '<!-- pre-update -->'
const PRE_UPDATE_END = '<!-- /pre-update -->'

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

function fragmentFilename(path) {
  const match = FILENAME.exec(basename(path))
  if (!match || match[0] !== basename(path))
    throw new Error(`${path}: expected <${TIERS.join('|')}>-<kind>-<name>.md`)
  return match
}

function isLocale(value) {
  return LOCALE.exec(value)?.[0] === value
}

function translationTarget(path) {
  const match = TRANSLATION.exec(basename(path))
  return match && match[0] === basename(path)
    ? { path: `${dirname(path)}/${match[1]}.md`, locale: match[2] }
    : null
}

function fragmentBody(path, body) {
  if (!body) throw new Error(`${path}: empty fragment`)
  if (
    /^\s{0,3}#{1,6}(?:\s|$)/m.test(body) ||
    /^\s{0,3}(?:=+|-+)\s*$/m.test(body)
  ) {
    throw new Error(`${path}: headings are not allowed`)
  }
  if (!/^[-+*]\s+\S/.test(body))
    throw new Error(`${path}: body must start with a Markdown bullet item`)
  return body
}

function parseFragment(path, data, requireMetadata = true) {
  const match = fragmentFilename(path)
  let body = decodeUtf8(data).trim()
  let releaseNotes = null
  const metadata =
    /^---\r?\n(release-notes|user-facing): (true|false)\r?\n---(?:\r?\n|$)/.exec(
      body,
    )
  if (metadata && (!requireMetadata || metadata[1] === 'release-notes')) {
    releaseNotes = metadata[2] === 'true'
    body = body.slice(metadata[0].length).trim()
  } else if (requireMetadata || body.startsWith('---')) {
    throw new Error(
      `${path}: expected frontmatter with release-notes: true or false`,
    )
  }
  return {
    path,
    tier: match[1],
    kind: match[2],
    data,
    body: fragmentBody(path, body),
    releaseNotes,
  }
}

function collectFragments(entries, requireMetadata = true) {
  const fragments = new Map()
  for (const [path, data] of entries) {
    if (translationTarget(path)) continue
    fragments.set(path, parseFragment(path, data, requireMetadata))
  }
  return fragments
}

function sameFragment(a, b) {
  return a.path === b.path && a.data.equals(b.data)
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
    fragments.set(path, readFileSync(path))
  }
  return collectFragments(fragments)
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
      regularBlob(entry.slice(0, separator), `${ref}:${path}`),
    )
  }
  return collectFragments(fragments, false)
}

function taggedFile(path, ref) {
  const entry = treeEntry(ref, path)
  return entry === null
    ? Buffer.alloc(0)
    : regularBlob(entry.split('\t')[0], `${ref}:${path}`)
}

function sourceFile(path, ref) {
  return ref
    ? taggedFile(path, ref)
    : existsSync(path)
      ? readFileSync(path)
      : Buffer.alloc(0)
}

function sourceFragments(product, ref) {
  return ref ? taggedFragments(product, ref) : workingFragments(product)
}

function originTags() {
  return memo('origin tags', () =>
    releaseTags(git('ls-remote', '--tags', 'origin')),
  )
}

function localTags() {
  return releaseTags(
    git(
      'for-each-ref',
      '--format=%(if)%(*objectname)%(then)%(*objectname)%(else)%(objectname)%(end)%09%(refname)',
      'refs/tags',
    ),
  )
}

function releaseTags(output) {
  const tags = new Map()
  for (const line of decodeUtf8(output).trim().split('\n')) {
    if (!line) continue
    let [sha, name] = line.split('\t')
    name = name.replace(/^refs\/tags\//, '')
    const peeled = name.endsWith('^{}')
    name = name.replace(/\^\{\}$/, '')
    const match = /^([a-z0-9]+(?:-[a-z0-9]+)*)\/v(.+)$/.exec(name)
    if (!match || !Object.hasOwn(MANIFESTS, match[1])) continue
    const [, prerelease] = parseVersion(match[1], match[2])
    if (match[1] === 'start-os' && prerelease?.match(/^rc\.[1-9][0-9]*$/))
      continue
    if (!tags.has(name) || peeled) tags.set(name, sha)
  }
  return tags
}

function compareText(a, b) {
  return a < b ? -1 : a > b ? 1 : 0
}

const memos = new Map()

function memo(key, compute) {
  if (!memos.has(key)) memos.set(key, compute())
  return memos.get(key)
}

function originReleases(ref, product = null, tags = originTags()) {
  const head = decodeUtf8(
    git('rev-parse', '--verify', `${ref}^{commit}`),
  ).trim()
  const positions = memo(`rev-list ${head}`, () => {
    const ordered = decodeUtf8(
      git('rev-list', '--topo-order', '--reverse', head),
    )
      .trim()
      .split('\n')
    return new Map(ordered.map((commit, index) => [commit, index]))
  })
  const releases = []
  for (const [tag, sha] of tags) {
    if (product && !tag.startsWith(`${basename(product)}/v`)) continue
    const commit = memo(`commit ${sha}`, () =>
      decodeUtf8(git('rev-parse', '--verify', `${sha}^{commit}`)).trim(),
    )
    if (!positions.has(commit)) continue
    const [name, version] = tag.split('/v')
    const path = `projects/${name}`
    releases.push({
      product: path,
      version,
      commit,
      fragments: memo(`fragments ${path} ${commit}`, () =>
        taggedFragments(path, commit),
      ),
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
          sameFragment(old, fragment) &&
          !deletedBetween(fragment.path, previous.commit, release.commit)
        )
      }),
  )
}

function fragmentLines(fragments) {
  return [...fragments]
    .sort((a, b) => compareText(a.path, b.path))
    .flatMap(fragment => [fragment.body, ''])
}

function releaseSection(version, fragments) {
  const lines = [`## [${version}]`, '']
  for (const kind of KINDS) {
    const group = fragments.filter(item => item.kind === kind)
    if (group.length)
      lines.push(
        `### ${kind[0].toUpperCase()}${kind.slice(1)}`,
        '',
        ...fragmentLines(group),
      )
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

function compiledSection(history, version) {
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
    return Buffer.from(text.slice(start, end))
  }
  return Buffer.alloc(0)
}

function prependSection(history, version, fragments, format) {
  if (!fragments.length) return history
  const section = releaseSection(version, fragments)
  const existing = compiledSection(history, version)
  if (existing.length) {
    if (!format(existing).equals(format(section))) {
      throw new Error(
        `release ${version} already has a different compiled changelog; do not move released tags`,
      )
    }
    return history
  }
  const text = decodeUtf8(history)
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
  const history = sourceFile(path, ref && commit)
  const fragments = sourceFragments(product, ref && commit)
  const releases = originReleases(commit, product).filter(
    item => item.version !== version,
  )
  releases.push({ product, version, commit, fragments })
  return compileReleases(history, releases)[0]
}

function notesPath(product, version, locale = ENGLISH) {
  return `${product}/release-notes/${version}${locale === ENGLISH ? '' : `.${locale}`}.md`
}

function archivedLocales(product, version, ref) {
  const locales = new Set()
  for (const path of notePaths(product, ref)) {
    const name = basename(path)
    if (
      dirname(path) !== `${product}/release-notes` ||
      !name.startsWith(`${version}.`) ||
      !name.endsWith('.md')
    )
      continue
    const locale = name.slice(version.length + 1, -3)
    if (isLocale(locale) && locale !== ENGLISH) locales.add(locale)
  }
  return [...locales].sort(compareText)
}

function notePaths(product, ref) {
  const directory = `${product}/release-notes`
  return ref
    ? decodeUtf8(
        git('ls-tree', '-r', '--name-only', '-z', ref, '--', directory),
      ).split('\0')
    : existsSync(directory)
      ? readdirSync(directory).map(name => `${directory}/${name}`)
      : []
}

function archivedNotePaths(product, ref = 'HEAD') {
  const paths = new Set()
  for (const release of originReleases(ref, product, localTags())) {
    for (const locale of [
      ENGLISH,
      ...archivedLocales(product, release.version, ref),
    ]) {
      const path = notesPath(product, release.version, locale)
      if (treeEntry(ref, path)) paths.add(path)
    }
  }
  return [...paths].sort(compareText)
}

function hasReleaseNotesMetadata(fragments) {
  return fragments.some(fragment => fragment.releaseNotes !== null)
}

function selectedByDefault(fragment) {
  return fragment.releaseNotes === true || fragment.kind === 'security'
}

function noteItems(fragments) {
  return [...fragments]
    .sort((a, b) => compareText(a.path, b.path))
    .map(({ path, kind, body }) => ({ path, kind, body }))
}

function localizedNotesSource(path, fallback, ref) {
  const notes = sourceFile(path, ref)
  return notes.length || path === fallback
    ? { path, notes }
    : { path: fallback, notes: sourceFile(fallback, ref) }
}

function sectionBounds(text, heading) {
  const match = new RegExp(
    `^${heading.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}[ \\t]*$`,
    'm',
  ).exec(text)
  if (!match) return null
  const body = match.index + match[0].length
  const next = /^## /m.exec(text.slice(body))
  return { start: match.index, end: next ? body + next.index : text.length }
}

function placeSection(text, heading, bodies, anchors) {
  if (!bodies.length) return text
  const block = bodies.join('\n\n')
  const bounds = sectionBounds(text, heading)
  const position = bounds
    ? bounds.end
    : (anchors.map(anchor => anchor(text)).find(index => index != null) ??
      text.length)
  const before = text.slice(0, position).trimEnd()
  const inserted = bounds ? block : `${heading}\n\n${block}`
  return `${before}${before ? '\n\n' : ''}${inserted}\n\n${text.slice(position)}`
}

function insertNotes(text, items, ref) {
  const labels = releaseNoteLabels(ref)
  const highlights = `## ${releaseNoteLabel(labels, 'highlights-title', ENGLISH)}`
  const security = `## ${releaseNoteLabel(labels, 'security-title', ENGLISH)}`
  const important = value => /^## Important[ \t]*$/m.exec(value)?.index
  const bodies = predicate =>
    items.filter(predicate).map(item => item.body.trim())
  text = placeSection(
    text,
    highlights,
    bodies(item => item.kind !== 'security'),
    [value => sectionBounds(value, security)?.start, important],
  )
  text = placeSection(
    text,
    security,
    bodies(item => item.kind === 'security'),
    [value => sectionBounds(value, highlights)?.end, important],
  )
  return text
}

function formatNotes(text) {
  const trimmed = text.trim()
  return trimmed
    ? markdownFormatter()(Buffer.from(trimmed + '\n'))
    : Buffer.alloc(0)
}

function compileNotes(path, notes, items, ref) {
  const text = decodeUtf8(notes).replace(/\r\n/g, '\n')
  const labels = releaseNoteLabels(ref)
  for (const key of ['highlights-title', 'security-title']) {
    const heading = `## ${releaseNoteLabel(labels, key, ENGLISH)}`
    if (sectionBounds(text, heading))
      throw new Error(
        `${path}: remove ${heading}; it is compiled from fragments`,
      )
  }
  return formatNotes(insertNotes(text, items, ref))
}

function renderNotes(product, version, ref) {
  parseVersion(basename(product), version)
  const path = notesPath(product, version)
  const notes = sourceFile(path, ref)
  const fragments = releaseFragments(product, version, ref)
  if (!hasReleaseNotesMetadata(fragments) && notes.length) return notes
  return compileNotes(
    path,
    notes,
    noteItems(fragments.filter(selectedByDefault)),
    ref,
  )
}

function releaseFragments(product, version, ref) {
  parseVersion(basename(product), version)
  const fragments = sourceFragments(product, ref)
  if (!fragments.size) return []
  const commit = decodeUtf8(
    git('rev-parse', '--verify', `${ref || 'HEAD'}^{commit}`),
  ).trim()
  const releases = originReleases(commit, product, localTags()).filter(
    release => release.version !== version,
  )
  return ownedFragments({ product, version, commit, fragments }, releases)
}

function noteCandidates(product, version, ref, since) {
  const earlier = since ? sourceFragments(product, since) : new Map()
  return releaseFragments(product, version, ref)
    .filter(
      fragment =>
        !earlier.get(fragment.path) ||
        !sameFragment(earlier.get(fragment.path), fragment),
    )
    .sort((a, b) => compareText(a.path, b.path))
    .map(fragment => ({
      path: fragment.path,
      kind: fragment.kind,
      body: fragment.body,
      selected: selectedByDefault(fragment),
    }))
}

function parseSelection(data, source) {
  const items = JSON.parse(decodeUtf8(data))
  if (
    !Array.isArray(items) ||
    items.some(
      item =>
        !KINDS.includes(item?.kind) ||
        typeof item.body !== 'string' ||
        !item.body.trim(),
    )
  )
    throw new Error(`${source}: expected an array of { kind, body }`)
  return items
}

function composeNotes(product, version, ref, selectionPath, basePath) {
  parseVersion(basename(product), version)
  const items = parseSelection(readFileSync(selectionPath), selectionPath)
  if (basePath === undefined) {
    const path = notesPath(product, version)
    return compileNotes(path, sourceFile(path, ref), items, ref)
  }
  return formatNotes(
    insertNotes(decodeUtf8(readFileSync(basePath)), items, ref),
  )
}

function preUpdateNotes(product, version, ref, locale = ENGLISH) {
  const path = notesPath(product, version, locale).replace(
    /\.md$/,
    '.pre-update.md',
  )
  const fallback = notesPath(product, version).replace(
    /\.md$/,
    '.pre-update.md',
  )
  return decodeUtf8(localizedNotesSource(path, fallback, ref).notes)
}

function composeReleaseNotes(notes, warning) {
  return warning.trim()
    ? `${PRE_UPDATE_START}\n\n${warning.trim()}\n\n${PRE_UPDATE_END}\n\n${notes}`
    : notes
}

function releaseNotes(product, version, ref, edited) {
  return composeReleaseNotes(
    edited?.[ENGLISH] ?? decodeUtf8(renderNotes(product, version, ref)),
    preUpdateNotes(product, version, ref),
  )
}

function parseNotesBundle(data, source) {
  let bundle
  try {
    bundle = JSON.parse(decodeUtf8(data))
  } catch (error) {
    throw new Error(`${source}: ${error.message}`)
  }
  if (
    !bundle ||
    typeof bundle !== 'object' ||
    Array.isArray(bundle) ||
    typeof bundle[ENGLISH] !== 'string' ||
    Object.entries(bundle).some(
      ([locale, notes]) => !isLocale(locale) || typeof notes !== 'string',
    )
  )
    throw new Error(
      `${source}: expected an object of locale -> notes with ${ENGLISH}`,
    )
  return bundle
}

function formatNotesBundle(path) {
  const format = markdownFormatter()
  const bundle = parseNotesBundle(readFileSync(path), path)
  const formatted = Object.fromEntries(
    Object.entries(bundle).map(([locale, notes]) => [
      locale,
      notes.trim() ? decodeUtf8(format(Buffer.from(notes))) : '',
    ]),
  )
  writeFileSync(path, `${JSON.stringify(formatted, null, 2)}\n`)
}

function readNotesBundle(path) {
  return path === undefined
    ? undefined
    : parseNotesBundle(readFileSync(path), path)
}

function releaseNoteLabels(ref) {
  return memo(`labels ${ref ?? ''}`, () => loadReleaseNoteLabels(ref))
}

function loadReleaseNoteLabels(ref) {
  const path = 'shared-libs/crates/start-core/locales/release-notes.json'
  const tagged = ref ? decodeUtf8(sourceFile(path, ref)) : ''
  const packaged = new URL(`../${path}`, import.meta.url)
  // Tag workflows can stage the compiler alone.
  const defaults = JSON.parse(
    readFileSync(existsSync(packaged) ? packaged : path),
  )
  return tagged ? { ...defaults, ...JSON.parse(tagged) } : defaults
}

function releaseNoteLabel(labels, key, locale) {
  const translations = labels[`release-notes.${key}`]
  return translations[locale] ?? translations[ENGLISH]
}

function changelogLink(product, version, ref, locale, labels) {
  const base = 'https://github.com/Start9Labs/start-technologies'
  let url = `${base}/releases/tag/${basename(product)}/v${version}`
  let pending = false
  if (ref) {
    pending =
      spawnSync('git', ['cat-file', '-e', `${ref}:${product}/changelog`])
        .status === 0
    const anchor = version.replaceAll('.', '').replaceAll('+', '').toLowerCase()
    url = pending
      ? `${base}/tree/${ref}/${product}/changelog`
      : `${base}/blob/${ref}/${product}/CHANGELOG.md#${anchor}`
  }
  const label = releaseNoteLabel(
    labels,
    pending ? 'full-changelog-fragments' : 'full-changelog',
    locale,
  )
  return label.replaceAll('%{version}', version).replaceAll('%{url}', url)
}

function placeChangelogLink(notes, link) {
  const lines = notes.split('\n')
  const output = []
  let dropped = false
  for (const line of lines) {
    if (line.startsWith('**[Full changelog')) {
      dropped = true
      continue
    }
    if (dropped) {
      dropped = false
      if (!line) continue
    }
    output.push(line)
  }
  const body = output.join('\n').trimEnd()
  return `${body}${body ? '\n\n' : ''}${link}\n`
}

function registryNotes(product, version, ref, changelogRef, edited) {
  const bundle = edited
    ? { ...edited }
    : { [ENGLISH]: decodeUtf8(renderNotes(product, version, ref)) }
  const labels = releaseNoteLabels(ref)
  for (const path of notePaths(product, ref)) {
    const prefix = `${product}/release-notes/${version}.`
    if (!path.startsWith(prefix) || !path.endsWith('.pre-update.md')) continue
    const locale = path.slice(prefix.length, -'.pre-update.md'.length)
    if (
      isLocale(locale) &&
      locale !== ENGLISH &&
      !Object.hasOwn(bundle, locale)
    )
      bundle[locale] = bundle[ENGLISH]
  }
  const composed = Object.fromEntries(
    Object.entries(bundle).map(([locale, notes]) => {
      return [
        locale,
        placeChangelogLink(
          composeReleaseNotes(
            notes,
            preUpdateNotes(product, version, ref, locale),
          ),
          changelogLink(product, version, changelogRef, locale, labels),
        ),
      ]
    }),
  )
  return {
    releaseNotes:
      Object.keys(composed).length === 1 ? composed[ENGLISH] : composed,
  }
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

function unchangedFragment(fragment, source) {
  return !(
    deletedBetween(fragment.path, source, 'HEAD') ||
    hasSymlink(fragment.path) ||
    !existsSync(fragment.path) ||
    !lstatSync(fragment.path).isFile() ||
    !readFileSync(fragment.path).equals(fragment.data)
  )
}

function githubRepository() {
  return process.env.GITHUB_REPOSITORY || '{owner}/{repo}'
}

function publishedNoteAssets() {
  return memo('published note assets', () => {
    const listing = decodeUtf8(
      run('gh', [
        'api',
        '--paginate',
        `repos/${githubRepository()}/releases`,
        '--jq',
        `.[] | .tag_name as $tag | .assets[] | select(.name == "${NOTES_ASSET}") | [$tag, .id] | @tsv`,
      ]),
    )
    return new Map(
      listing
        .split('\n')
        .filter(Boolean)
        .map(line => line.split('\t')),
    )
  })
}

function publishedNotes(product, version) {
  const tag = `${basename(product)}/v${version}`
  const id = publishedNoteAssets().get(tag)
  if (!id) return null
  return parseNotesBundle(
    run('gh', [
      'api',
      '-H',
      'Accept: application/octet-stream',
      `repos/${githubRepository()}/releases/assets/${id}`,
    ]),
    `${tag}: ${NOTES_ASSET}`,
  )
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
    for (const release of releases) {
      if (release.product !== product) continue
      const bundle = publishedNotes(product, release.version)
      if (!bundle) continue
      for (const [locale, notes] of Object.entries(bundle)) {
        const path = notesPath(product, release.version, locale)
        if (hasSymlink(path))
          throw new Error(`${path}: symlinks are not allowed`)
        const existing = existsSync(path) ? readFileSync(path) : Buffer.alloc(0)
        updates.push({
          path,
          history: existing,
          compiled: Buffer.from(notes),
          consumed: [],
        })
      }
    }
    updates.push({ path, history, compiled, consumed })
  }
  for (const { path, history, compiled, consumed } of updates) {
    if (!compiled.equals(history)) {
      mkdirSync(dirname(path), { recursive: true })
      writeFileSync(path, compiled)
    }
    for (const [source, fragment] of consumed) {
      if (!unchangedFragment(fragment, source)) continue
      unlinkSync(fragment.path)
    }
  }
  for (const product of new Set(releases.map(release => release.product))) {
    const directory = `${product}/changelog`
    if (!existsSync(directory)) continue
    for (const name of readdirSync(directory)) {
      const target = translationTarget(`${directory}/${name}`)
      if (target && !existsSync(target.path)) unlinkSync(`${directory}/${name}`)
    }
  }
}

const USAGE =
  'usage: changelog.mjs projects | changed BASE | validate PRODUCT | version PRODUCT | check-version PRODUCT [VERSION] | render PRODUCT VERSION [--ref REF] | notes PRODUCT VERSION [--ref REF] | security PRODUCT VERSION [--ref REF] | candidates PRODUCT VERSION [--ref REF] [--since REF] | compose-notes PRODUCT VERSION --selection FILE [--ref REF] [--base FILE] | release-notes PRODUCT VERSION [--ref REF] [--notes FILE] | registry-notes PRODUCT VERSION [--ref REF] [--changelog-ref REF] [--notes FILE] | archives PRODUCT [--ref REF] | format-notes FILE | section VERSION | sync'

try {
  const { values, positionals } = parseArgs({
    allowPositionals: true,
    options: {
      ref: { type: 'string' },
      since: { type: 'string' },
      selection: { type: 'string' },
      base: { type: 'string' },
      'changelog-ref': { type: 'string' },
      notes: { type: 'string' },
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
    notes: [2],
    security: [2],
    candidates: [2],
    'compose-notes': [2],
    'release-notes': [2],
    'registry-notes': [2],
    archives: [1],
    section: [1],
    'format-notes': [1],
    sync: [0],
  }
  if (
    !values.help &&
    (!counts[command]?.includes(args.length) ||
      (values.ref !== undefined &&
        ![
          'render',
          'notes',
          'security',
          'candidates',
          'compose-notes',
          'release-notes',
          'registry-notes',
          'archives',
        ].includes(command)) ||
      (values['changelog-ref'] !== undefined && command !== 'registry-notes') ||
      (values.notes !== undefined &&
        !['release-notes', 'registry-notes'].includes(command)) ||
      (values.since !== undefined && command !== 'candidates') ||
      (values.selection === undefined) !== (command !== 'compose-notes') ||
      (values.base !== undefined && command !== 'compose-notes'))
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
  else if (command === 'notes')
    process.stdout.write(renderNotes(productPath(args[0]), args[1], values.ref))
  else if (command === 'security')
    console.log(
      releaseFragments(productPath(args[0]), args[1], values.ref).some(
        fragment => fragment.kind === 'security',
      ),
    )
  else if (command === 'candidates')
    console.log(
      JSON.stringify(
        noteCandidates(productPath(args[0]), args[1], values.ref, values.since),
      ),
    )
  else if (command === 'compose-notes')
    process.stdout.write(
      composeNotes(
        productPath(args[0]),
        args[1],
        values.ref,
        values.selection,
        values.base,
      ),
    )
  else if (command === 'release-notes')
    process.stdout.write(
      releaseNotes(
        productPath(args[0]),
        args[1],
        values.ref,
        readNotesBundle(values.notes),
      ),
    )
  else if (command === 'registry-notes')
    console.log(
      JSON.stringify(
        registryNotes(
          productPath(args[0]),
          args[1],
          values.ref,
          values['changelog-ref'],
          readNotesBundle(values.notes),
        ),
      ),
    )
  else if (command === 'archives')
    process.stdout.write(
      archivedNotePaths(productPath(args[0]), values.ref)
        .map(path => `${path}\n`)
        .join(''),
    )
  else if (command === 'format-notes') formatNotesBundle(args[0])
  else if (command === 'section')
    process.stdout.write(compiledSection(readFileSync(0), args[0]))
  else sync()
} catch (error) {
  process.stderr.write(`changelog: ${error.message}\n`)
  process.exitCode = 1
}
