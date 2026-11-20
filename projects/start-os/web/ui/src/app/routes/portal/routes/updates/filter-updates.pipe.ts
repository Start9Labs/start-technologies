import { inject, Pipe, PipeTransform } from '@angular/core'
import { MarketplacePkg, pendingReleases } from '@start9labs/marketplace'
import { Exver } from '@start9labs/shared'
import {
  InstalledState,
  PackageDataEntry,
  UpdatingState,
} from 'src/app/services/patch-db/data-model'

@Pipe({
  name: 'filterUpdates',
})
export class FilterUpdatesPipe implements PipeTransform {
  private readonly exver = inject(Exver)

  transform(
    pkgs: MarketplacePkg[],
    local: Record<string, PackageDataEntry> = {},
    hidden: Record<string, string[]> = {},
  ): MarketplacePkg[] {
    return pkgs.filter(({ id, flavor, version }) => {
      const localPkg = local[id]
      return (
        !!localPkg &&
        (localPkg.stateInfo.state === 'installed' ||
          localPkg.stateInfo.state === 'updating') &&
        this.exver.getFlavor(localPkg.stateInfo.manifest.version) === flavor &&
        this.exver.compareExver(
          version,
          localPkg.stateInfo.manifest.version,
        ) === 1 &&
        !hidden[id]?.includes(version)
      )
    })
  }
}

@Pipe({
  name: 'hasSecurityUpdate',
})
export class HasSecurityUpdatePipe implements PipeTransform {
  private readonly exver = inject(Exver)

  transform(
    updates: MarketplacePkg[],
    local: Record<
      string,
      PackageDataEntry<InstalledState | UpdatingState>
    > = {},
  ): boolean {
    return updates.some(pkg =>
      pendingReleases(
        this.exver,
        pkg,
        local[pkg.id]?.stateInfo.manifest.version,
      ).some(({ security }) => security),
    )
  }
}
