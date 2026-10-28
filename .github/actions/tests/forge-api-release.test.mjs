import assert from 'node:assert/strict'
import { mkdtemp, writeFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { test } from 'node:test'
import { createBranchApi, createRepositoryApi } from '../lib/forge-api.mjs'
import { publishRelease } from '../forgejo-release/release.mjs'

for (const apiUrl of [
  'https://api.github.com',
  'https://forge.example/api/v1',
]) {
  test(`repository API targets ${apiUrl} with the caller token`, async () => {
    const calls = []
    const request = createRepositoryApi({
      apiUrl,
      repository: 'owner/package',
      token: 'test-token',
      fetchImpl: async (...args) => {
        calls.push(args)
        return Response.json({ default_branch: 'main' })
      },
    })
    const api = createBranchApi(request, 'owner/package')
    assert.equal(await api.defaultBranch(), 'main')
    assert.equal(calls[0][0], `${apiUrl}/repos/owner/package`)
    assert.equal(calls[0][1].headers.Authorization, 'token test-token')
  })
}

test('pull request lookup paginates and excludes other branches and forks', async () => {
  const unrelated = {
    base: { ref: 'next' },
    head: { ref: 'sync-next/master', repo: { full_name: 'fork/package' } },
  }
  const urls = []
  const request = async (method, path) => {
    urls.push(path)
    return urls.length === 1
      ? Array(50).fill(unrelated)
      : [
          { ...unrelated, base: { ref: 'wrong' } },
          {
            ...unrelated,
            head: {
              ref: 'sync-next/master',
              repo: { full_name: 'owner/package' },
            },
          },
        ]
  }
  const api = createBranchApi(request, 'owner/package')
  assert.equal(await api.hasOpenPullRequest('next', 'sync-next/master'), true)
  assert.deepEqual(urls, [
    '/pulls?state=open&per_page=50&limit=50&page=1',
    '/pulls?state=open&per_page=50&limit=50&page=2',
  ])
  assert.equal(
    await createBranchApi(async () => [], 'owner/package').hasOpenPullRequest(
      'next',
      'scratch',
    ),
    false,
  )
})

test('API failures propagate, and only an allowed 404 means missing', async () => {
  for (const status of [401, 403, 500]) {
    const request = createRepositoryApi({
      apiUrl: 'https://forge',
      repository: 'o/r',
      token: 't',
      fetchImpl: async () => new Response('failure', { status }),
    })
    await assert.rejects(
      request('GET', '/releases/tags/v1', { allowNotFound: true }),
      new RegExp(`HTTP ${status}`),
    )
  }
  const missing = createRepositoryApi({
    apiUrl: 'https://forge',
    repository: 'o/r',
    token: 't',
    fetchImpl: async () => new Response('', { status: 404 }),
  })
  assert.equal(await missing('GET', '/release', { allowNotFound: true }), null)
  await assert.rejects(missing('GET', '/release'), /HTTP 404/)
})

test('Forgejo release updates in place and replaces only same-named assets', async t => {
  const dir = await mkdtemp(join(tmpdir(), 'release-test-'))
  t.after(() => rm(dir, { recursive: true, force: true }))
  const file = join(dir, 'package x86.s9pk')
  await writeFile(file, 's9pk contents')
  const calls = []
  const release = {
    id: 7,
    assets: [
      { id: 10, name: 'package x86.s9pk' },
      { id: 11, name: 'other.s9pk' },
    ],
  }
  const request = createRepositoryApi({
    apiUrl: 'https://forge/api/v1',
    repository: 'o/r',
    token: 't',
    fetchImpl: async (url, options) => {
      calls.push({ url, ...options })
      return options.method === 'DELETE'
        ? new Response(null, { status: 204 })
        : Response.json(release)
    },
  })
  await publishRelease({
    request,
    tag: 'v1/arch',
    name: '1.0',
    body: 'notes',
    files: [file],
  })
  assert.deepEqual(
    calls.map(call => [
      call.method,
      call.url.replace('https://forge/api/v1/repos/o/r', ''),
    ]),
    [
      ['GET', '/releases/tags/v1%2Farch'],
      ['PATCH', '/releases/7'],
      ['DELETE', '/releases/7/assets/10'],
      ['POST', '/releases/7/assets?name=package%20x86.s9pk'],
    ],
  )
  assert.deepEqual(JSON.parse(calls[1].body), {
    tag_name: 'v1/arch',
    name: '1.0',
    body: 'notes',
  })
  const attachment = calls[3].body.get('attachment')
  assert.equal(attachment.name, 'package x86.s9pk')
  assert.equal(await attachment.text(), 's9pk contents')
  assert.equal(calls[3].headers['Content-Type'], undefined)
})

test('missing release is created and supports notes without assets', async () => {
  const calls = []
  const request = async (...args) => {
    calls.push(args)
    return args[0] === 'GET' ? null : { id: 1 }
  }
  await publishRelease({
    request,
    tag: 'v1',
    name: '1',
    body: 'notes',
    files: [],
  })
  assert.deepEqual(calls, [
    ['GET', '/releases/tags/v1', { allowNotFound: true }],
    [
      'POST',
      '/releases',
      { json: { tag_name: 'v1', name: '1', body: 'notes' } },
    ],
  ])
})

test('release upload failures fail the job without deleting the release', async t => {
  const dir = await mkdtemp(join(tmpdir(), 'release-test-'))
  t.after(() => rm(dir, { recursive: true, force: true }))
  const file = join(dir, 'package.s9pk')
  await writeFile(file, 'package')
  const calls = []
  const request = async (method, path) => {
    calls.push([method, path])
    if (method === 'POST') throw new Error('upload failed')
    return { id: 7, assets: [] }
  }
  await assert.rejects(
    publishRelease({ request, tag: 'v1', name: '1', body: '', files: [file] }),
    /upload failed/,
  )
  assert.equal(
    calls.some(([method]) => method === 'DELETE'),
    false,
  )
})
