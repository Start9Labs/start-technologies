import { execFileSync, spawnSync } from 'node:child_process'

export async function syncNext({ cwd, base, forge, botName, botEmail }) {
  function git(...args) {
    return execFileSync('git', args, { cwd, encoding: 'utf8' }).trim()
  }
  function succeeds(...args) {
    const result = spawnSync('git', args, { cwd, encoding: 'utf8' })
    if (result.error) throw result.error
    if (result.status !== 0 && result.status !== 1) {
      throw new Error(result.stderr)
    }
    return result.status === 0
  }
  const exists = branch =>
    succeeds('show-ref', '--verify', '--quiet', `refs/remotes/origin/${branch}`)
  const contains = (ancestor, descendant) =>
    succeeds('merge-base', '--is-ancestor', ancestor, descendant)

  git('config', 'user.name', botName)
  git('config', 'user.email', botEmail)
  git(
    'fetch',
    '--no-tags',
    '--prune',
    'origin',
    '+refs/heads/*:refs/remotes/origin/*',
  )
  if (!exists(base))
    throw new Error(
      `No branch '${base}' on this repository — check the caller workflow.`,
    )

  const defaultBranch = await forge.defaultBranch()
  const target =
    exists(`next/${base}`) || base !== defaultBranch ? `next/${base}` : 'next'
  const baseSha = git('rev-parse', `origin/${base}`)
  if (!exists(target)) {
    git('push', 'origin', `${baseSha}:refs/heads/${target}`)
    return 'created'
  }
  const targetSha = git('rev-parse', `origin/${target}`)
  if (contains(baseSha, targetSha)) return 'unchanged'
  if (contains(targetSha, baseSha)) {
    git('push', 'origin', `${baseSha}:refs/heads/${target}`)
    return 'fast-forwarded'
  }

  git('checkout', '-B', 'sync-next-work', `origin/${target}`)
  if (
    succeeds(
      'merge',
      '--no-edit',
      '-m',
      `chore: merge ${base} into ${target}`,
      `origin/${base}`,
    )
  ) {
    git('push', 'origin', `HEAD:refs/heads/${target}`)
    return 'merged'
  }
  git('merge', '--abort')

  const scratch = `sync-next/${base}`
  if (await forge.hasOpenPullRequest(target, scratch))
    return 'pull-request-exists'
  git('push', '--force', 'origin', `${baseSha}:refs/heads/${scratch}`)
  await forge.createPullRequest({
    base: target,
    head: scratch,
    title: `chore: sync ${base} into ${target}`,
    body: `\`${base}\` has moved on and cannot be merged into \`${target}\` unattended — the merge conflicts.

Resolve the conflicts and merge this PR. Until it lands, \`${target}\` is missing what \`${base}\` has gained, and work built on it will be based on a stale tree.

**Merge it with a merge commit, not a squash.** \`${target}\` has to come out with \`${base}\` as an ancestor; a squash gives it equivalent content under a fresh commit instead, and the next sync diverges again for the same reason this one did.

\`${target}\` and \`${base}\` are both untouched — nothing was pushed to either and no history was rewritten. \`${scratch}\` is a throwaway branch holding the \`${base}\` tip, and it is deleted when this merges.`,
  })
  return 'pull-request-created'
}
