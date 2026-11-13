import { computed, Directive, inject, input } from '@angular/core'
import { toObservable, toSignal } from '@angular/core/rxjs-interop'
import { IconDirective } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { startWith, switchMap } from 'rxjs'
import { PackageIconService } from 'src/app/services/package-icon.service'
import { PackageDataEntry } from 'src/app/services/patch-db/data-model'

@Directive({
  selector: 'img[pkgIcon]',
  exportAs: 'pkgIcon',
})
export class PkgIconDirective extends IconDirective {
  private readonly icons = inject(PackageIconService)

  public readonly pkgIcon = input.required<
    PackageDataEntry | T.ProviderPin | string
  >()
  public readonly dependency = input<string>()

  protected readonly src = toSignal(
    toObservable(
      computed(() => this.icons.get(this.pkgIcon(), this.dependency())),
    ).pipe(switchMap(icon$ => icon$.pipe(startWith(null)))),
    { initialValue: null },
  )
}
