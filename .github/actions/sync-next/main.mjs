import { createBranchApi, createRepositoryApi } from '../lib/forge-api.mjs'
import { syncNext } from './sync-next.mjs'

const forgejo = process.env.FORGEJO_ACTIONS === 'true'
const apiUrl = forgejo
  ? `${process.env.GITHUB_SERVER_URL}/api/v1`
  : process.env.GITHUB_API_URL
syncNext({
  cwd: process.cwd(),
  base: process.env.BASE,
  botName: forgejo ? 'forgejo-actions[bot]' : 'github-actions[bot]',
  botEmail: forgejo
    ? `forgejo-actions@${new URL(process.env.GITHUB_SERVER_URL).hostname}`
    : '41898282+github-actions[bot]@users.noreply.github.com',
  forge: createBranchApi(
    createRepositoryApi({
      apiUrl,
      repository: process.env.GITHUB_REPOSITORY,
      token: process.env.FORGE_TOKEN,
    }),
    process.env.GITHUB_REPOSITORY,
  ),
})
  .then(result => console.log(`Sync next: ${result}`))
  .catch(error => {
    console.error(error)
    process.exitCode = 1
  })
