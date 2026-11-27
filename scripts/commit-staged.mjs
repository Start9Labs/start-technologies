#!/usr/bin/env node
import { spawnSync } from 'node:child_process'
import { readFileSync } from 'node:fs'

const utf8 = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true })
const MODES = ['000000', '100644', '100755', '120000']

function git(...args) {
  const result = spawnSync('git', args, { maxBuffer: Infinity })
  if (result.error) throw result.error
  if (result.status !== 0) throw new Error(utf8.decode(result.stderr))
  return result.stdout
}

function api(method, endpoint, body) {
  const args = ['api', '--method', method]
  args.push(`repos/${process.env.GITHUB_REPOSITORY}/${endpoint}`)
  if (body) args.push('--input', '-')
  const result = spawnSync('gh', args, {
    input: body && JSON.stringify(body),
    maxBuffer: Infinity,
  })
  if (result.error) throw result.error
  const output = utf8.decode(result.stdout)
  if (result.status !== 0) throw new Error(output + utf8.decode(result.stderr))
  return JSON.parse(output)
}

try {
  const [messagePath, branch, expectedHead] = process.argv.slice(2)
  const changes = []
  const rows = utf8
    .decode(
      git('diff', '--cached', '--raw', '-z', '--no-renames', expectedHead),
    )
    .split('\0')
  for (let index = 0; index < rows.length - 1; index += 2) {
    const [oldMode, mode, , sha] = rows[index].slice(1).split(' ')
    const path = rows[index + 1]
    if (!MODES.includes(oldMode) || !MODES.includes(mode)) {
      throw Object.assign(
        new Error(`Commit API cannot represent the file mode of ${path}`),
        { exitCode: 2 },
      )
    }
    changes.push({ path, oldMode, mode, sha })
  }
  if (changes.length) {
    const message = utf8
      .decode(readFileSync(messagePath))
      .replace(/\r\n?/g, '\n')
    const tree = changes.map(({ path, oldMode, mode, sha }) =>
      mode === '000000'
        ? { path, mode: oldMode, type: 'blob', sha: null }
        : {
            path,
            mode,
            type: 'blob',
            sha: api('POST', 'git/blobs', {
              content: git('cat-file', 'blob', sha).toString('base64'),
              encoding: 'base64',
            }).sha,
          },
    )
    const base = api('GET', `git/commits/${expectedHead}`).tree.sha
    const commit = api('POST', 'git/commits', {
      message,
      tree: api('POST', 'git/trees', { base_tree: base, tree }).sha,
      parents: [expectedHead],
    })
    if (commit.verification?.verified !== true) {
      throw Object.assign(
        new Error(
          `GitHub did not verify ${commit.sha}: ${JSON.stringify(commit.verification)}`,
        ),
        { exitCode: 3 },
      )
    }
    api('PATCH', `git/refs/heads/${branch}`, { sha: commit.sha, force: false })
    console.log(commit.sha)
  }
} catch (error) {
  console.error(error.message)
  process.exitCode = error.exitCode ?? 1
}
