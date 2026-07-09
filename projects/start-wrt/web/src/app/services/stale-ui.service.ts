import { inject, Injectable, signal } from '@angular/core'
import { GIT_HASH } from 'src/app/utils/workspace-config'
import { SystemInfoRes } from './api/api.service'

/**
 * A real build stamp: full git hash, optionally "-modified" (dirty tree).
 * Excludes the dev GIT_BRANCH_AS_HASH "@branch" stamp and the "unknown"
 * fallback, which can't be meaningfully compared.
 */
function comparable(hash: string | undefined): hash is string {
  return !!hash && /^[0-9a-f]{40}(-modified)?$/.test(hash)
}

/**
 * Ignore the "-modified" marker: a clean and a dirty build of the same commit
 * compare equal. Dirtiness alone must not nag — the dev deploy loop rebuilds
 * from the same HEAD constantly.
 */
function base(hash: string): string {
  return hash.replace(/-modified$/, '')
}

/**
 * Detects a stale cached UI bundle. The firmware reports its build stamp in
 * `system.info` (`gitHash`) and the web build bakes the identical stamp into
 * `config.json` (the GIT_HASH token) — build-rust.sh, ctrl's build.rs, and
 * check-git-hash.sh all emit the same format, so divergence means this tab is
 * running a bundle from a previous firmware. Where the divergence is observed
 * decides the UX: the in-tab update flow reloads outright (SystemService),
 * while passive observers only raise `stale` and StaleUiAlert prompts for a
 * reload — never forcing one out from under unsaved work.
 */
@Injectable({ providedIn: 'root' })
export class StaleUiService {
  private readonly bundleHash = inject(GIT_HASH)

  /**
   * Latches true once any system.info response reports a different build than
   * this bundle. Never resets — only a reload can un-stale the bundle.
   */
  readonly stale = signal(false)

  isStale(info: SystemInfoRes): boolean {
    return (
      comparable(this.bundleHash) &&
      comparable(info.gitHash) &&
      base(this.bundleHash) !== base(info.gitHash)
    )
  }

  check(info: SystemInfoRes): void {
    if (this.isStale(info)) {
      this.stale.set(true)
    }
  }
}
