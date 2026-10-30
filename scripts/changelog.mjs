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
const HIGHLIGHTS = /^## Highlights[ \t]*\r?$/m
const LOCALE_PATTERN = '[a-z]{2}_[A-Z]{2}'
const LOCALE = new RegExp(`^${LOCALE_PATTERN}$`)
const TRANSLATION = new RegExp(`^(.+)\\.(${LOCALE_PATTERN})\\.md$`)
const HIGHLIGHTS_MARKER = '<!-- highlights -->'
const ENGLISH = 'en_US'

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
  let userFacing = null
  const metadata = /^---\r?\nuser-facing: (true|false)\r?\n---(?:\r?\n|$)/.exec(
    body,
  )
  if (metadata) {
    userFacing = metadata[1] === 'true'
    body = body.slice(metadata[0].length).trim()
  } else if (requireMetadata || body.startsWith('---')) {
    throw new Error(
      `${path}: expected frontmatter with user-facing: true or false`,
    )
  }
  return {
    path,
    tier: match[1],
    kind: match[2],
    data,
    body: fragmentBody(path, body),
    userFacing,
    translations: new Map(),
  }
}

function collectFragments(entries, requireMetadata = true) {
  const fragments = new Map()
  const translations = []
  for (const [path, data] of entries) {
    const target = translationTarget(path)
    if (target) {
      fragmentFilename(target.path)
      if (target.locale === ENGLISH)
        throw new Error(`${path}: English belongs in the canonical fragment`)
      translations.push({
        ...target,
        source: path,
        data,
        body: fragmentBody(path, decodeUtf8(data).trim()),
      })
    } else fragments.set(path, parseFragment(path, data, requireMetadata))
  }
  for (const { path, source, locale, data, body } of translations) {
    const fragment = fragments.get(path)
    if (!fragment)
      throw new Error(`${source}: missing canonical fragment ${path}`)
    fragment.translations.set(locale, { path: source, data, body })
  }
  return fragments
}

function fragmentFiles(fragment) {
  return [fragment, ...fragment.translations.values()]
}

function sameFragment(a, b) {
  const files = new Map(fragmentFiles(a).map(file => [file.path, file.data]))
  return (
    files.size === fragmentFiles(b).length &&
    fragmentFiles(b).every(file => files.get(file.path)?.equals(file.data))
  )
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
          !fragmentFiles(fragment).some(file =>
            deletedBetween(file.path, previous.commit, release.commit),
          )
        )
      }),
  )
}

function fragmentLines(fragments, locale = ENGLISH) {
  return [...fragments]
    .sort((a, b) => compareText(a.path, b.path))
    .flatMap(fragment => [
      fragment.translations.get(locale)?.body ?? fragment.body,
      '',
    ])
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

function noteLocales(product, version, fragments, ref) {
  const locales = new Set(
    fragments
      .filter(fragment => fragment.userFacing)
      .flatMap(fragment => [...fragment.translations.keys()]),
  )
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

function locales(product, version, ref) {
  parseVersion(basename(product), version)
  return noteLocales(
    product,
    version,
    [...sourceFragments(product, ref).values()],
    ref,
  )
}

function archivedNotePaths(product, ref = 'HEAD') {
  const paths = new Set()
  for (const release of originReleases(ref, product, localTags())) {
    for (const locale of [
      ENGLISH,
      ...noteLocales(
        product,
        release.version,
        [...release.fragments.values()],
        ref,
      ),
    ]) {
      const path = notesPath(product, release.version, locale)
      if (treeEntry(ref, path)) paths.add(path)
    }
  }
  return [...paths].sort(compareText)
}

function hasAudienceMetadata(fragments) {
  return fragments.some(fragment => fragment.userFacing !== null)
}

function localizedNotesSource(path, fallback, ref) {
  const notes = sourceFile(path, ref)
  return notes.length || path === fallback
    ? { path, notes }
    : { path: fallback, notes: sourceFile(fallback, ref) }
}

function noteFraming(product, version, locale, ref) {
  return localizedNotesSource(
    notesPath(product, version, locale),
    notesPath(product, version, ENGLISH),
    ref,
  )
}

function compileNotes(path, notes, fragments, locale = ENGLISH, ref) {
  const text = decodeUtf8(notes)
  const parts = text.split(HIGHLIGHTS_MARKER)
  if (!hasAudienceMetadata(fragments) && parts.length === 1 && notes.length)
    return notes
  const labels = releaseNoteLabels(ref)
  const visible = fragments.filter(fragment => fragment.userFacing)
  const security = visible.filter(fragment => fragment.kind === 'security')
  const securityHeading = `## ${releaseNoteLabel(labels, 'security-title', locale)}`
  const englishSecurityHeading = `## ${releaseNoteLabel(labels, 'security-title', ENGLISH)}`
  if (
    text
      .split(/\r?\n/)
      .some(line =>
        [securityHeading, englishSecurityHeading].includes(line.trimEnd()),
      )
  )
    throw new Error(
      `${path}: remove ${securityHeading}; it is compiled from fragments`,
    )
  const lines = [
    ...fragmentLines(
      visible.filter(fragment => fragment.kind !== 'security'),
      locale,
    ),
    ...(security.length
      ? [securityHeading, '', ...fragmentLines(security, locale)]
      : []),
  ]
  let composed
  if (parts.length > 2)
    throw new Error(`${path}: expected one ${HIGHLIGHTS_MARKER}`)
  const heading =
    parts.length === 2
      ? [...parts[0].matchAll(/^## [^\r\n]+\r?$/gm)].at(-1)
      : null
  const authored = HIGHLIGHTS.exec(text)
  if (authored && authored.index !== heading?.index)
    throw new Error(
      `${path}: remove ## Highlights; it is compiled from fragments`,
    )
  if (parts.length === 2) {
    const [before, after] = parts
    const next = /^## /m.exec(after)
    if (
      !heading ||
      before.slice(heading.index + heading[0].length).trim() ||
      after.slice(0, next?.index ?? after.length).trim()
    )
      throw new Error(
        `${path}: ${HIGHLIGHTS_MARKER} must occupy an empty section`,
      )
    composed = `${before.trimEnd()}\n\n${lines.join('\n')}\n${after.trimStart()}`
  } else {
    if (translationTarget(path) && notes.length)
      throw new Error(`${path}: localized framing needs ${HIGHLIGHTS_MARKER}`)
    const highlights = [
      `## ${releaseNoteLabel(labels, 'highlights-title', locale)}`,
      '',
      ...lines,
    ].join('\n')
    const important = /^## Important[ \t]*\r?$/m.exec(text)
    const position = important ? important.index : text.length
    composed = `${text.slice(0, position).trimEnd()}\n\n${highlights}\n${text.slice(position)}`
  }
  return markdownFormatter()(Buffer.from(composed.trim() + '\n'))
}

function renderNotes(product, version, ref, locale = ENGLISH) {
  parseVersion(basename(product), version)
  const { path, notes } = noteFraming(product, version, locale, ref)
  return compileNotes(
    path,
    notes,
    releaseFragments(product, version, ref),
    locale,
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

function renderNotesBundle(product, version, ref) {
  return Object.fromEntries(
    [ENGLISH, ...locales(product, version, ref)].map(locale => [
      locale,
      decodeUtf8(renderNotes(product, version, ref, locale)),
    ]),
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
  return warning ? `${warning}\n\n${notes}` : notes
}

function releaseNotes(product, version, ref) {
  return composeReleaseNotes(
    decodeUtf8(renderNotes(product, version, ref)),
    preUpdateNotes(product, version, ref),
  )
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

function registryNotes(product, version, ref, changelogRef) {
  const bundle = renderNotesBundle(product, version, ref)
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
      bundle[locale] = decodeUtf8(renderNotes(product, version, ref, locale))
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
  const files = fragmentFiles(fragment)
  for (const file of files) {
    if (
      deletedBetween(file.path, source, 'HEAD') ||
      hasSymlink(file.path) ||
      !existsSync(file.path) ||
      !lstatSync(file.path).isFile() ||
      !readFileSync(file.path).equals(file.data)
    )
      return false
  }
  const paths = new Set(files.map(file => file.path))
  return readdirSync(dirname(fragment.path)).every(name => {
    const path = `${dirname(fragment.path)}/${name}`
    return translationTarget(path)?.path !== fragment.path || paths.has(path)
  })
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
    for (const [index, release] of releases.entries()) {
      if (release.product !== product) continue
      const fragments = ownedFragments(release, releases.slice(0, index))
      if (!hasAudienceMetadata(fragments)) continue
      for (const locale of [
        ENGLISH,
        ...noteLocales(product, release.version, fragments, release.commit),
      ]) {
        const path = notesPath(product, release.version, locale)
        if (hasSymlink(path))
          throw new Error(`${path}: symlinks are not allowed`)
        const source = taggedFile(path, release.commit)
        const framing = noteFraming(
          product,
          release.version,
          locale,
          release.commit,
        )
        const compiled = compileNotes(
          framing.path,
          framing.notes,
          fragments,
          locale,
          release.commit,
        )
        const existing = existsSync(path) ? readFileSync(path) : Buffer.alloc(0)
        if (
          existing.length &&
          !existing.equals(compiled) &&
          !existing.equals(source)
        )
          throw new Error(
            `${path}: release notes differ from the tagged source and compiled output`,
          )
        updates.push({ path, history: existing, compiled, consumed: [] })
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
      for (const file of fragmentFiles(fragment)) unlinkSync(file.path)
    }
  }
}

const USAGE =
  'usage: changelog.mjs projects | changed BASE | validate PRODUCT | version PRODUCT | check-version PRODUCT [VERSION] | render PRODUCT VERSION [--ref REF] | notes PRODUCT VERSION [--ref REF] [--locale LOCALE] | security PRODUCT VERSION [--ref REF] | notes-bundle PRODUCT VERSION [--ref REF] | release-notes PRODUCT VERSION [--ref REF] | registry-notes PRODUCT VERSION [--ref REF] [--changelog-ref REF] | locales PRODUCT VERSION [--ref REF] | archives PRODUCT [--ref REF] | section VERSION | sync'

try {
  const { values, positionals } = parseArgs({
    allowPositionals: true,
    options: {
      ref: { type: 'string' },
      locale: { type: 'string' },
      'changelog-ref': { type: 'string' },
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
    'notes-bundle': [2],
    'release-notes': [2],
    'registry-notes': [2],
    locales: [2],
    archives: [1],
    section: [1],
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
          'notes-bundle',
          'release-notes',
          'registry-notes',
          'locales',
          'archives',
        ].includes(command)) ||
      (values['changelog-ref'] !== undefined && command !== 'registry-notes') ||
      (values.locale !== undefined &&
        (command !== 'notes' || !isLocale(values.locale))))
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
    process.stdout.write(
      renderNotes(productPath(args[0]), args[1], values.ref, values.locale),
    )
  else if (command === 'security')
    console.log(
      releaseFragments(productPath(args[0]), args[1], values.ref).some(
        fragment => fragment.kind === 'security',
      ),
    )
  else if (command === 'notes-bundle')
    console.log(
      JSON.stringify(
        renderNotesBundle(productPath(args[0]), args[1], values.ref),
      ),
    )
  else if (command === 'release-notes')
    process.stdout.write(
      releaseNotes(productPath(args[0]), args[1], values.ref),
    )
  else if (command === 'registry-notes')
    console.log(
      JSON.stringify(
        registryNotes(
          productPath(args[0]),
          args[1],
          values.ref,
          values['changelog-ref'],
        ),
      ),
    )
  else if (command === 'locales')
    console.log(locales(productPath(args[0]), args[1], values.ref).join(' '))
  else if (command === 'archives')
    process.stdout.write(
      archivedNotePaths(productPath(args[0]), values.ref)
        .map(path => `${path}\n`)
        .join(''),
    )
  else if (command === 'section')
    process.stdout.write(compiledSection(readFileSync(0), args[0]))
  else sync()
} catch (error) {
  process.stderr.write(`changelog: ${error.message}\n`)
  process.exitCode = 1
}
