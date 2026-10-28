export function createRepositoryApi({
  apiUrl,
  repository,
  token,
  fetchImpl = fetch,
}) {
  return async function request(
    method,
    path,
    { json, body, allowNotFound = false } = {},
  ) {
    const response = await fetchImpl(`${apiUrl}/repos/${repository}${path}`, {
      method,
      headers: {
        Authorization: `token ${token}`,
        Accept: 'application/json',
        ...(json === undefined ? {} : { 'Content-Type': 'application/json' }),
      },
      body: json === undefined ? body : JSON.stringify(json),
    })
    if (allowNotFound && response.status === 404) return null
    if (!response.ok) {
      throw new Error(
        `${method} ${path}: HTTP ${response.status} ${await response.text()}`,
      )
    }
    return response.status === 204 ? undefined : response.json()
  }
}

export function createBranchApi(request, repository) {
  return {
    async defaultBranch() {
      return (await request('GET', '')).default_branch
    },
    async hasOpenPullRequest(base, head) {
      for (let page = 1; ; page++) {
        const pulls = await request(
          'GET',
          `/pulls?state=open&per_page=50&limit=50&page=${page}`,
        )
        if (
          pulls.some(
            pull =>
              pull.base.ref === base &&
              pull.head.ref === head &&
              pull.head.repo?.full_name === repository,
          )
        ) {
          return true
        }
        if (pulls.length < 50) return false
      }
    },
    createPullRequest: json => request('POST', '/pulls', { json }),
  }
}
