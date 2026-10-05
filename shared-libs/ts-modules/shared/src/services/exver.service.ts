import { Service } from '@angular/core'
import { ExtendedVersion, VersionRange } from '@start9labs/start-core'

@Service()
export class Exver {
  compareExver(lhs: string, rhs: string): number | null {
    if (!lhs || !rhs) return null
    try {
      return ExtendedVersion.parse(lhs).compareForSort(
        ExtendedVersion.parse(rhs),
      )
    } catch (e) {
      return null
    }
  }

  greaterThanOrEqual(lhs: string, rhs: string): boolean | null {
    if (!lhs || !rhs) return null
    try {
      return ExtendedVersion.parse(lhs).greaterThanOrEqual(
        ExtendedVersion.parse(rhs),
      )
    } catch (e) {
      return null
    }
  }

  satisfies(version: string, range: string): boolean {
    return ExtendedVersion.parse(version).satisfies(VersionRange.parse(range))
  }

  releaseSatisfies(
    version: string,
    satisfies: string[],
    range: string,
  ): boolean {
    return VersionRange.parse(range).satisfiedByRelease(
      [version, ...satisfies].map(v => ExtendedVersion.parse(v)),
    )
  }

  getFlavor(version: string): string | null {
    return ExtendedVersion.parse(version).flavor
  }
}
