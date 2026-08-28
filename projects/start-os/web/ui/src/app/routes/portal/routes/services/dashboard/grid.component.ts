import { Component, computed, inject, input } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { RouterLink } from '@angular/router'
import { i18nPipe } from '@start9labs/shared'
import { TuiCell, TuiTitle } from '@taiga-ui/core'
import { TuiAvatar, TuiSkeleton } from '@taiga-ui/kit'
import { TuiCardLarge } from '@taiga-ui/layout'
import { DepErrorService } from 'src/app/services/dep-error.service'
import { PackageDataEntry } from 'src/app/services/patch-db/data-model'
import { getManifest } from 'src/app/utils/get-package-data'
import { byName } from './sorters'
import { ServiceTileComponent } from './tile.component'

@Component({
  selector: '[servicesGrid]',
  template: `
    @for (service of sorted(); track manifest(service).id) {
      @let id = manifest(service).id;

      <a
        [appServiceTile]="service"
        [depErrors]="errors()?.[id] || {}"
        [routerLink]="'/services/' + id"
      ></a>
    } @empty {
      @for (_ of '-'.repeat(6); track $index) {
        <div tuiCardLarge="compact" [tuiSkeleton]="true">
          <span tuiCell>
            <span tuiAvatar></span>
            <span tuiTitle>
              {{ 'Loading' | i18n }}
              <span tuiSubtitle>{{ 'Loading' | i18n }}</span>
            </span>
          </span>
        </div>
      }
    }
  `,
  styles: `
    :host {
      display: grid;
      gap: 1rem;
      grid-template-columns: repeat(auto-fill, minmax(17rem, 1fr));
    }
  `,
  imports: [
    RouterLink,
    ServiceTileComponent,
    TuiAvatar,
    TuiCardLarge,
    TuiCell,
    TuiSkeleton,
    TuiTitle,
    i18nPipe,
  ],
})
export class ServicesGridComponent {
  private readonly depErrors = inject(DepErrorService)

  readonly services = input.required<readonly PackageDataEntry[] | null>({
    alias: 'servicesGrid',
  })
  readonly asc = input.required<boolean>()

  protected readonly errors = toSignal(this.depErrors.depErrors$)
  protected readonly manifest = getManifest
  protected readonly sorted = computed(() => {
    const direction = this.asc() ? 1 : -1

    return [...(this.services() || [])].sort((a, b) => byName(a, b) * direction)
  })
}
