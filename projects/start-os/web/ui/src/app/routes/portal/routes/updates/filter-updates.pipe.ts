import { inject, Pipe, PipeTransform } from '@angular/core'
import { MarketplaceListing } from '@start9labs/marketplace'
import { Exver } from '@start9labs/shared'
import { PackageDataEntry } from 'src/app/services/patch-db/data-model'

@Pipe({
  name: 'filterUpdates',
})
export class FilterUpdatesPipe implements PipeTransform {
  private readonly exver = inject(Exver)

  transform(
    pkgs: MarketplaceListing[],
    local: Record<string, PackageDataEntry> = {},
    hidden: Record<string, string[]> = {},
  ): MarketplaceListing[] {
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
