import { inject, Pipe, PipeTransform } from '@angular/core'
import { PackageIconService } from 'src/app/services/package-icon.service'
import { PackageDataEntry } from 'src/app/services/patch-db/data-model'

@Pipe({
  name: 'pkgIcon',
})
export class PkgIconPipe implements PipeTransform {
  private readonly icons = inject(PackageIconService)

  transform(pkg: PackageDataEntry | string, dependency?: string) {
    return this.icons.get(pkg, dependency)
  }
}
