import { createBranchApi, createRepositoryApi } from '../lib/forge-api.mjs'
import { syncNext } from './sync-next.mjs'

const forge =
  process.env.FORGEJO_ACTIONS === 'true'
    ? 'forgejo'
    : process.env.GITEA_ACTIONS === 'true'
      ? 'gitea'
      : 'github'
const apiUrl =
  forge === 'github'
    ? process.env.GITHUB_API_URL
    : `${process.env.GITHUB_SERVER_URL}/api/v1`
syncNext({
  cwd: process.cwd(),
  base: process.env.BASE,
  botName: `${forge}-actions[bot]`,
  botEmail:
    forge === 'github'
      ? '41898282+github-actions[bot]@users.noreply.github.com'
      : `${forge}-actions@${new URL(process.env.GITHUB_SERVER_URL).hostname}`,
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
