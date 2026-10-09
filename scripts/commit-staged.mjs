#!/usr/bin/env node
import { spawnSync } from 'node:child_process'
import { readFileSync } from 'node:fs'

const utf8 = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true })

function git(...args) {
  const result = spawnSync('git', args, { maxBuffer: Infinity })
  if (result.error) throw result.error
  if (result.status !== 0) throw new Error(utf8.decode(result.stderr))
  return result.stdout
}

try {
  const [messagePath, branch, expectedHead] = process.argv.slice(2)
  const additions = []
  const deletions = []
  const rows = utf8
    .decode(
      git('diff', '--cached', '--raw', '-z', '--no-renames', expectedHead),
    )
    .split('\0')
  for (let index = 0; index < rows.length - 1; index += 2) {
    const metadata = rows[index].split(/\s+/)
    const path = rows[index + 1]
    const oldMode = metadata[0].slice(1)
    const newMode = metadata[1]
    if (
      !['000000', '100644'].includes(oldMode) ||
      !['000000', '100644'].includes(newMode)
    ) {
      throw Object.assign(
        new Error(`Commit API cannot represent the file mode of ${path}`),
        { exitCode: 2 },
      )
    }
    if (newMode === '000000') deletions.push({ path })
    else
      additions.push({
        path,
        contents: git('show', `:${path}`).toString('base64'),
      })
  }
  if (additions.length || deletions.length) {
    const message = utf8
      .decode(readFileSync(messagePath))
      .replace(/\r\n?/g, '\n')
    const newline = message.indexOf('\n')
    const headline = newline === -1 ? message : message.slice(0, newline)
    const body = newline === -1 ? '' : message.slice(newline + 1).trim()
    const payload = {
      query:
        'mutation($input: CreateCommitOnBranchInput!) { createCommitOnBranch(input: $input) { commit { oid url } } }',
      variables: {
        input: {
          branch: {
            repositoryNameWithOwner: process.env.GITHUB_REPOSITORY,
            branchName: branch,
          },
          expectedHeadOid: expectedHead,
          message: { headline, body },
          fileChanges: { additions, deletions },
        },
      },
    }
    const result = spawnSync('gh', ['api', 'graphql', '--input', '-'], {
      input: JSON.stringify(payload),
      maxBuffer: Infinity,
    })
    if (result.error) throw result.error
    const output = utf8.decode(result.stdout)
    if (result.status !== 0)
      throw new Error(output + utf8.decode(result.stderr))
    const response = JSON.parse(output)
    if (response.errors?.length) throw new Error(output)
    console.log(response.data.createCommitOnBranch.commit.oid)
  }
} catch (error) {
  console.error(error.message)
  process.exitCode = error.exitCode ?? 1
}
