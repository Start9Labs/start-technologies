import { readFile } from 'node:fs/promises'
import { createRepositoryApi } from '../lib/forge-api.mjs'
import { publishRelease } from './release.mjs'

publishRelease({
  request: createRepositoryApi({
    apiUrl: `${process.env.GITHUB_SERVER_URL}/api/v1`,
    repository: process.env.GITHUB_REPOSITORY,
    token: process.env.FORGE_TOKEN,
  }),
  tag: process.env.RELEASE_TAG,
  name: process.env.RELEASE_NAME,
  body: await readFile(process.env.RELEASE_BODY_PATH, 'utf8'),
  files: (process.env.RELEASE_FILES || '').split(/\r?\n/).filter(Boolean),
}).catch(error => {
  console.error(error)
  process.exitCode = 1
})
