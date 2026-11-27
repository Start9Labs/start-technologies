import { T } from '@start9labs/start-core'

export type FoundVulnerability = T.Vulnerability & { id: string }

/** Trusted vulnerabilities whose affected range the version satisfies. */
export function affecting(
  published: Record<string, T.Vulnerability> | undefined,
  trusted: readonly string[] | undefined,
  satisfies: (affected: string) => boolean,
): FoundVulnerability[] {
  return (trusted ?? []).flatMap(id => {
    const vulnerability = published?.[id]
    return vulnerability && satisfies(vulnerability.affected)
      ? [{ ...vulnerability, id }]
      : []
  })
}
