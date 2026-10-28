import { readFileSync } from 'node:fs'
import { join, extname } from 'node:path'

export const MANIFESTS = Object.freeze({
  'start-os': 'package.json',
  'start-sdk': 'projects/start-sdk/package.json',
  'start-cli': 'projects/start-cli/Cargo.toml',
  'start-tunnel': 'projects/start-tunnel/Cargo.toml',
  'start-registry': 'projects/start-registry/Cargo.toml',
  'start-wrt': 'projects/start-wrt/backend/ctrl/Cargo.toml',
})
export const TIERS = Object.freeze(['patch', 'minor', 'major'])
const NUMBER = '(?:0|[1-9][0-9]*)'
const PRERELEASE_ID = '(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)'
const SUFFIX = `(?:-(?<prerelease>${PRERELEASE_ID}(?:\\.${PRERELEASE_ID})*))?(?:\\+[0-9A-Za-z-]+(?:\\.[0-9A-Za-z-]+)*)?`
const SEMVER = new RegExp(`^(?<core>${NUMBER}(?:\\.${NUMBER}){2})${SUFFIX}$`)
const STARTOS_VERSION = new RegExp(`^(?<core>0(?:\\.${NUMBER}){2,3})${SUFFIX}$`)

export function decodeUtf8(data) {
  return new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(data)
}

function requireProject(project) {
  if (!Object.hasOwn(MANIFESTS, project)) {
    throw new Error(`unknown release project: ${JSON.stringify(project)}`)
  }
}

export function parseVersion(project, version) {
  requireProject(project)
  const pattern = project === 'start-os' ? STARTOS_VERSION : SEMVER
  const match = typeof version === 'string' ? pattern.exec(version) : null
  if (!match || match[0] !== version) {
    throw new Error(`invalid ${project} version: ${JSON.stringify(version)}`)
  }
  let numbers = match.groups.core.split('.').map(BigInt)
  if (project === 'start-os') {
    numbers = numbers.slice(1)
    if (numbers.length === 2) numbers.push(0n)
  }
  return [numbers, match.groups.prerelease ?? null]
}

export function nextVersion(project, releasedVersion, tiers) {
  let [[major, minor, patch], prerelease] = parseVersion(
    project,
    releasedVersion,
  )
  if (prerelease !== null) {
    throw new Error(
      `release baseline must be stable: ${JSON.stringify(releasedVersion)}`,
    )
  }
  let impact = -1
  for (const tier of tiers) {
    const index = TIERS.indexOf(tier)
    if (index < 0)
      throw new Error(`invalid fragment tier: ${JSON.stringify(tier)}`)
    impact = Math.max(impact, index)
  }
  if (impact === -1) return releasedVersion
  if (impact === 2) [major, minor, patch] = [major + 1n, 0n, 0n]
  else if (impact === 1) [minor, patch] = [minor + 1n, 0n]
  else patch += 1n
  if (project === 'start-os')
    return `0.${major}.${minor}${patch ? `.${patch}` : ''}`
  return `${major}.${minor}.${patch}`
}

export function latestRelease(project, tagNames) {
  requireProject(project)
  const prefix = `${project}/v`
  let latest = null
  let latestNumbers = null
  for (const tag of tagNames) {
    if (!tag.startsWith(prefix)) continue
    const version = tag.slice(prefix.length)
    let numbers, prerelease
    try {
      ;[numbers, prerelease] = parseVersion(project, version)
    } catch {
      continue
    }
    const difference =
      latestNumbers === null
        ? 0
        : numbers.findIndex((value, index) => value !== latestNumbers[index])
    if (
      prerelease === null &&
      (latestNumbers === null ||
        (difference >= 0 && numbers[difference] > latestNumbers[difference]))
    ) {
      latest = version
      latestNumbers = numbers
    }
  }
  return latest
}

export function manifestVersion(repoRoot, project) {
  requireProject(project)
  const path = join(repoRoot, MANIFESTS[project])
  const text = decodeUtf8(readFileSync(path))
  let version
  if (extname(path) === '.json') {
    version = JSON.parse(text)?.version
  } else {
    let inPackage = false
    const versions = []
    for (const line of text.split(/\r\n?|\n/)) {
      if (line.trimStart().startsWith('[')) {
        inPackage = /^\s*\[\s*package\s*\]\s*(?:#.*)?$/.test(line)
      } else if (inPackage && /^\s*version\s*(?:=|\.)/.test(line)) {
        const value = /^\s*version\s*=\s*(["'])([^"']*)\1\s*(?:#.*)?$/.exec(
          line,
        )
        if (!value)
          throw new Error(`${path}: expected a literal package version`)
        versions.push(value[2])
      }
    }
    if (versions.length !== 1)
      throw new Error(`${path}: expected one [package].version`)
    version = versions[0]
  }
  parseVersion(project, version)
  return version
}
