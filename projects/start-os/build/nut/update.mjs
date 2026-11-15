#!/usr/bin/env node
import { execFileSync, spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { basename, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

export const POOL = 'https://deb.debian.org/debian/pool/main/n/nut/'
export const RELEASES =
  'https://api.github.com/repos/networkupstools/nut/releases'
export const PINS = 'projects/start-os/build/nut/version.env'
export const CHANGELOG = 'projects/start-os/changelog'
const UPSTREAM = /^[0-9]+\.[0-9]+\.[0-9]+$/
const DEBIAN = /^([0-9]+\.[0-9]+\.[0-9]+)-[0-9][0-9A-Za-z.+~]*$/
const sha256 = data => createHash('sha256').update(data).digest('hex')

function version(value) {
  if (!UPSTREAM.test(value))
    throw Error(`Unsupported upstream version: ${value}`)
  return value.split('.').map(Number)
}

function compare(left, right) {
  const a = version(left),
    b = version(right)
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return a[i] - b[i]
  return 0
}

export async function fetchBytes(url, limit = 4 * 1024 * 1024) {
  const headers = { 'User-Agent': 'StartOS-NUT-updater' }
  if (process.env.GH_TOKEN && url.startsWith(RELEASES))
    headers.Authorization = `Bearer ${process.env.GH_TOKEN}`
  const response = await fetch(url, {
    headers,
    signal: AbortSignal.timeout(60_000),
  })
  if (!response.ok) throw Error(`HTTP ${response.status}: ${url}`)
  const chunks = []
  let size = 0
  for await (const chunk of response.body) {
    size += chunk.length
    if (size > limit) throw Error(`Response exceeds limit: ${url}`)
    chunks.push(chunk)
  }
  return Buffer.concat(chunks)
}

export async function archiveHash(url, expectedSize, request = fetch) {
  if (
    !Number.isSafeInteger(expectedSize) ||
    expectedSize <= 0 ||
    expectedSize > 512 * 1024 * 1024
  )
    throw Error('Source archive has an unsupported size')
  const response = await request(url, { signal: AbortSignal.timeout(60_000) })
  if (!response.ok) throw Error(`HTTP ${response.status}: ${url}`)
  const digest = createHash('sha256')
  let size = 0
  for await (const chunk of response.body) {
    size += chunk.length
    if (size > expectedSize)
      throw Error(`Source archive exceeds signed size: ${url}`)
    digest.update(chunk)
  }
  if (size !== expectedSize)
    throw Error(`Source archive size does not match signed manifest: ${url}`)
  return digest.digest('hex')
}

export function readPins(text) {
  const pins = Object.create(null)
  for (const line of text.split('\n')) {
    if (!line.trim() || line.startsWith('#')) continue
    const match =
      /^(NUT_[A-Z_0-9]+)=(?:'([^']*)'|"([^"\\$`]*)"|([^\s'"\\$`]+))$/.exec(line)
    if (!match || Object.hasOwn(pins, match[1]))
      throw Error('Malformed or duplicate NUT pin')
    pins[match[1]] = match[2] ?? match[3] ?? match[4]
  }
  return pins
}

export function chooseCandidate(index, pins, latest) {
  let selected = pins.NUT_DEBIAN_VERSION
  for (const match of index.matchAll(
    /<a\b[^>]*\bhref\s*=\s*["']nut_([^"']+)\.dsc["']/gi,
  )) {
    const candidate = match[1],
      source = DEBIAN.exec(candidate)
    if (!source || compare(source[1], latest) > 0) continue
    const result = spawnSync('dpkg', [
      '--compare-versions',
      candidate,
      'gt',
      selected,
    ])
    if (result.error) throw result.error
    if (![0, 1].includes(result.status))
      throw Error('Debian version comparison failed')
    if (result.status === 0) selected = candidate
  }
  return selected
}

export function verifyManifest(signed, signer, key) {
  if (!/^[A-F0-9]{40}$/.test(signer))
    throw Error('NUT_SIGNER must be a full fingerprint')
  const directory = mkdtempSync(join(tmpdir(), 'nut-signature-'))
  try {
    const path = name => join(directory, name)
    writeFileSync(path('key.asc'), key)
    writeFileSync(path('source.dsc'), signed)
    execFileSync(
      'gpg',
      [
        '--batch',
        '--no-tty',
        '--homedir',
        directory,
        '--dearmor',
        '--output',
        path('key.gpg'),
        path('key.asc'),
      ],
      { stdio: 'pipe' },
    )
    const status = execFileSync(
      'gpgv',
      [
        '--homedir',
        directory,
        '--keyring',
        path('key.gpg'),
        '--status-fd',
        '1',
        '--output',
        path('verified'),
        path('source.dsc'),
      ],
      { encoding: 'utf8', stdio: 'pipe' },
    )
    const signatures = status
      .split('\n')
      .filter(line => line.startsWith('[GNUPG:] VALIDSIG '))
      .map(line => line.split(' '))
    if (
      !signatures.some(
        fields => fields[2] === signer || fields.at(-1) === signer,
      )
    )
      throw Error('Source manifest was not signed by the pinned maintainer')
    return readFileSync(path('verified'), 'utf8')
  } finally {
    rmSync(directory, { recursive: true, force: true })
  }
}

export function sourceFiles(text, candidate, packages) {
  const fields = Object.create(null)
  let current
  for (const line of text.split('\n')) {
    if (/^[ \t]/.test(line) && current) fields[current] += '\n' + line.trim()
    else if (line) {
      const match = /^([^:]+):(.*)$/.exec(line)
      if (!match || Object.hasOwn(fields, match[1]))
        throw Error('Malformed or duplicate source manifest field')
      current = match[1]
      fields[current] = match[2].trim()
    }
  }
  if (
    fields.Source !== 'nut' ||
    fields.Version !== candidate ||
    fields.Format !== '3.0 (quilt)'
  )
    throw Error('Signed source identity or format changed; review manually')
  const binaries = new Set(
    (fields.Binary ?? '').split(',').map(name => name.trim()),
  )
  if (!packages.split(/\s+/).every(name => binaries.has(name)))
    throw Error('NUT runtime package names changed; review manually')
  const entries = Object.create(null)
  for (const line of (fields['Checksums-Sha256'] ?? '').split('\n')) {
    if (!line) continue
    const parts = line.trim().split(/\s+/)
    if (
      parts.length !== 3 ||
      !/^[a-f0-9]{64}$/.test(parts[0]) ||
      !/^[0-9]+$/.test(parts[1]) ||
      basename(parts[2]) !== parts[2] ||
      Object.hasOwn(entries, parts[2])
    )
      throw Error('Malformed source checksum entry')
    entries[parts[2]] = { hash: parts[0], size: Number(parts[1]) }
  }
  const upstream = DEBIAN.exec(candidate)?.[1]
  const names = [
    `nut_${upstream}.orig.tar.gz`,
    `nut_${candidate}.debian.tar.xz`,
  ]
  if (names.some(name => !Object.hasOwn(entries, name)))
    throw Error('Source archive format changed; review manually')
  return Object.fromEntries(names.map(name => [name, entries[name]]))
}

export async function prepare(
  root,
  get = fetchBytes,
  hashArchive = archiveHash,
) {
  if (!existsSync(join(root, PINS)))
    return {
      report: {
        changed: false,
        available: false,
        reason: 'NUT package pins are absent; waiting for package integration.',
      },
      changes: {},
    }
  const original = readFileSync(join(root, PINS), 'utf8'),
    pins = readPins(original)
  version(pins.NUT_VERSION)
  if (!DEBIAN.test(pins.NUT_DEBIAN_VERSION))
    throw Error('Unsupported pinned Debian version')
  const release = JSON.parse(await get(RELEASES + '/latest'))
  const latest = release.tag_name.replace(/^v/, '')
  version(latest)
  if (release.draft || release.prerelease)
    throw Error('Latest upstream release is not stable')
  const candidate = chooseCandidate((await get(POOL)).toString(), pins, latest),
    upstream = DEBIAN.exec(candidate)[1]
  const report = {
    current: pins.NUT_DEBIAN_VERSION,
    latestUpstream: latest,
    candidate,
    changed: candidate !== pins.NUT_DEBIAN_VERSION,
    waitingForDebian: compare(latest, upstream) > 0,
  }
  if (!report.changed) return { report, changes: {} }
  if (compare(upstream, pins.NUT_VERSION) < 0)
    throw Error('Refusing an upstream downgrade')
  const signed = await get(POOL + `nut_${candidate}.dsc`)
  const key = await get(
    `https://keyring.debian.org/pks/lookup?op=get&search=0x${pins.NUT_SIGNER}`,
  )
  const files = sourceFiles(
    verifyManifest(signed, pins.NUT_SIGNER, key),
    candidate,
    pins.NUT_PACKAGES,
  )
  for (const [name, { hash, size }] of Object.entries(files))
    if ((await hashArchive(POOL + name, size)) !== hash)
      throw Error(`Source archive checksum mismatch: ${name}`)
  const matching =
    upstream === latest
      ? release
      : JSON.parse(await get(RELEASES + '/tags/v' + upstream))
  if (
    matching.draft ||
    matching.prerelease ||
    matching.tag_name !== 'v' + upstream
  )
    throw Error('Matching upstream release is not stable')
  const assets = matching.assets.filter(
    asset => asset.name === `nut-${upstream}.tar.gz`,
  )
  const origHash = files[`nut_${upstream}.orig.tar.gz`].hash
  if (assets.length !== 1 || assets[0].digest !== 'sha256:' + origHash)
    throw Error(
      'Upstream release digest is missing or differs from Debian source; review manually',
    )
  const suffix = pins.NUT_PACKAGE_VERSION.startsWith(pins.NUT_DEBIAN_VERSION)
    ? /^(~startos[0-9]+)\.[0-9]+$/.exec(
        pins.NUT_PACKAGE_VERSION.slice(pins.NUT_DEBIAN_VERSION.length),
      )
    : null
  if (!suffix) throw Error('Unsupported StartOS package revision scheme')
  const updates = {
    NUT_VERSION: upstream,
    NUT_DEBIAN_VERSION: candidate,
    NUT_PACKAGE_VERSION: candidate + suffix[1] + '.1',
    NUT_DSC_SHA256: sha256(signed),
    NUT_SOURCE_SHA256: origHash,
    NUT_DEBIAN_SHA256: files[`nut_${candidate}.debian.tar.xz`].hash,
  }
  let rendered = original
  for (const [name, value] of Object.entries(updates)) {
    let count = 0
    rendered = rendered.replace(new RegExp('^' + name + '=.*$', 'gm'), () => {
      count++
      return name + '=' + value
    })
    if (count !== 1) throw Error(`Missing or duplicate pin: ${name}`)
  }
  const changes = { [PINS]: rendered },
    pending = `${CHANGELOG}/minor-added-ups-monitor.md`
  if (existsSync(join(root, pending))) {
    const text = readFileSync(join(root, pending), 'utf8'),
      marker = `Includes NUT ${pins.NUT_VERSION} for broader UPS compatibility.`
    if (text.split(marker).length !== 2)
      throw Error('Pending UPS feature fragment changed; review manually')
    const changed = text.replace(
      marker,
      `Includes NUT ${upstream} for broader UPS compatibility.`,
    )
    if (changed !== text) changes[pending] = changed
  } else {
    const entry =
      upstream !== pins.NUT_VERSION
        ? `Update UPS support to NUT ${upstream}.`
        : `Refresh NUT ${upstream} with Debian maintenance updates.`
    changes[`${CHANGELOG}/patch-changed-nut-update.md`] =
      `---\nrelease-notes: false\n---\n\n- ${entry}\n`
  }
  Object.assign(report, {
    packageVersion: updates.NUT_PACKAGE_VERSION,
    releaseUrl: `https://github.com/networkupstools/nut/releases/tag/v${upstream}`,
    files: Object.keys(changes),
  })
  return { report, changes }
}

async function main() {
  const args = process.argv.slice(2)
  let write = false,
    reportPath
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--write') write = true
    else if (args[i] === '--report' && args[i + 1]) reportPath = args[++i]
    else throw Error('Usage: update.mjs [--write] [--report path]')
  }
  const root = fileURLToPath(new URL('../../../../', import.meta.url))
  const { report, changes } = await prepare(root)
  if (write)
    for (const [name, contents] of Object.entries(changes))
      writeFileSync(join(root, name), contents)
  const rendered = JSON.stringify(report, null, 2) + '\n'
  if (reportPath) writeFileSync(reportPath, rendered)
  process.stdout.write(rendered)
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
)
  main().catch(error => {
    process.stderr.write(
      `NUT update verification failed: ${error.message}\n${error.stderr?.toString() ?? ''}`,
    )
    process.exitCode = 1
  })
