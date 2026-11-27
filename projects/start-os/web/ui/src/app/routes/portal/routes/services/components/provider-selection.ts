import { Component, signal } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { T } from '@start9labs/start-core'
import { i18nPipe } from '@start9labs/shared'
import { TuiButton, TuiCell, TuiDialogContext, TuiTitle } from '@taiga-ui/core'
import { TuiAvatar, TuiRadioList, TuiSkeleton } from '@taiga-ui/kit'
import { TuiForm, TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { PkgIconDirective } from 'src/app/routes/portal/components/pkg-icon'
import { Params } from 'src/app/services/api/api.types'
import { PackageDataEntry } from 'src/app/services/patch-db/data-model'
import { dependencyProviderChoices } from 'src/app/utils/dependency-provider'
import { ToManifestPipe } from '../../../pipes/to-manifest'

export type ProviderSelectionValue = Required<
  Pick<Params<'package.select-dependency-provider'>, 'providerId' | 'locked'>
>

export interface ProviderSelectionData {
  providerId: string | null
  pin: T.ProviderPin | null
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
          [ngModel]="providerId()"
          (ngModelChange)="chooseProvider($event)"
        />
        <ng-template #provider let-id>
          @let pkg = context.data.services[id];
          @let pin = id === context.data.providerId ? context.data.pin : null;
          @if (pkg || pin; as source) {
            <span tuiCell>
              <span tuiAvatar [round]="false" [tuiSkeleton]="icon.loading()">
                <img #icon="pkgIcon" [pkgIcon]="source" />
              </span>
              <span tuiTitle>
                {{ pkg ? (pkg | toManifest).title : pin?.title }}
                <span tuiSubtitle>
                  @if (context.data.candidates[id]; as versions) {
                    {{ versions.join(', ') }}
                  } @else {
                    {{
                      (pkg
                        ? 'No compatible version available'
                        : 'Not installed'
                      ) | i18n
                    }}
                  }
                </span>
              </span>
            </span>
          }
        </ng-template>
      } @else {
        <p>{{ 'No compatible version available' | i18n }}</p>
      }
      <footer>
        @if (providerId()) {
          <button
            tuiIconButton
            type="button"
            appearance="flat-grayscale"
            [iconStart]="locked() ? '@tui.lock' : '@tui.lock-open'"
            [attr.aria-pressed]="locked()"
            [disabled]="!locked() && !context.data.candidates[providerId()!]"
            (click)="locked.set(!locked())"
          >
            {{ (locked() ? 'Unlock provider' : 'Lock provider') | i18n }}
          </button>
        }
        <button
          tuiButton
          appearance="flat"
          type="button"
          (click)="context.$implicit.complete()"
        >
          {{ 'Cancel' | i18n }}
        </button>
        <button tuiButton [disabled]="!canSelect()">
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
    injectContext<
      TuiDialogContext<ProviderSelectionValue, ProviderSelectionData>
    >()
  protected readonly providers = dependencyProviderChoices(
    this.context.data.candidates,
    this.context.data.pin ? this.context.data.providerId : null,
  )
  protected readonly providerId = signal(
    this.context.data.providerId &&
      this.providers.includes(this.context.data.providerId)
      ? this.context.data.providerId
      : null,
  )
  protected readonly locked = signal(!!this.context.data.pin)

  protected chooseProvider(providerId: string) {
    if (providerId !== this.providerId()) {
      this.providerId.set(providerId)
      this.locked.set(false)
    }
  }

  protected canSelect() {
    const providerId = this.providerId()
    return (
      !!providerId &&
      (!!this.context.data.candidates[providerId] ||
        (!this.locked() &&
          !!this.context.data.pin &&
          providerId === this.context.data.providerId))
    )
  }

  protected select() {
    if (this.canSelect()) {
      this.context.completeWith({
        providerId: this.providerId(),
        locked: this.locked(),
      })
    }
  }
}

export const PROVIDER_SELECTION = new PolymorpheusComponent(ProviderSelection)
