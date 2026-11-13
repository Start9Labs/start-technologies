import { Component, signal } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { i18nPipe } from '@start9labs/shared'
import { TuiButton, TuiCell, TuiDialogContext, TuiTitle } from '@taiga-ui/core'
import { TuiAvatar, TuiRadioList, TuiSkeleton } from '@taiga-ui/kit'
import { TuiForm, TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { PkgIconDirective } from 'src/app/routes/portal/components/pkg-icon'
import { PackageDataEntry } from 'src/app/services/patch-db/data-model'
import { ToManifestPipe } from '../../../pipes/to-manifest'

export interface ProviderSelectionData {
  providerId: string | null
  candidates: Record<string, string[]>
  services: Record<string, PackageDataEntry>
}

@Component({
  template: `
    <form tuiForm="m" (submit.prevent)="select()">
      <header tuiHeader>
        <h2 tuiTitle [id]="context.id">
          {{ 'Select a dependency provider' | i18n }}
        </h2>
      </header>
      @if (providers.length) {
        <tui-radio-list
          name="providerId"
          [items]="providers"
          [itemContent]="provider"
          [(ngModel)]="providerId"
        />
        <ng-template #provider let-id>
          @if (context.data.services[id]; as pkg) {
            <span tuiCell>
              <span tuiAvatar [round]="false" [tuiSkeleton]="icon.loading()">
                <img #icon="pkgIcon" [pkgIcon]="pkg" />
              </span>
              <span tuiTitle>
                {{ (pkg | toManifest).title }}
                <span tuiSubtitle>
                  {{ context.data.candidates[id]?.join(', ') }}
                </span>
              </span>
            </span>
          }
        </ng-template>
      } @else {
        <p>{{ 'No compatible version available' | i18n }}</p>
      }
      <footer>
        <button
          tuiButton
          appearance="flat"
          type="button"
          (click)="context.$implicit.complete()"
        >
          {{ 'Cancel' | i18n }}
        </button>
        <button tuiButton [disabled]="!providerId()">
          {{ 'Select' | i18n }}
        </button>
      </footer>
    </form>
  `,
  imports: [
    FormsModule,
    i18nPipe,
    TuiButton,
    TuiCell,
    TuiTitle,
    TuiAvatar,
    TuiSkeleton,
    TuiRadioList,
    TuiForm,
    TuiHeader,
    PkgIconDirective,
    ToManifestPipe,
  ],
})
export class ProviderSelection {
  protected readonly context =
    injectContext<TuiDialogContext<string, ProviderSelectionData>>()
  protected readonly providers = Object.keys(this.context.data.candidates)
  protected readonly providerId = signal(
    this.context.data.providerId &&
      this.providers.includes(this.context.data.providerId)
      ? this.context.data.providerId
      : null,
  )

  protected select() {
    const providerId = this.providerId()
    if (providerId && this.context.data.candidates[providerId])
      this.context.completeWith(providerId)
  }
}

export const PROVIDER_SELECTION = new PolymorpheusComponent(ProviderSelection)
