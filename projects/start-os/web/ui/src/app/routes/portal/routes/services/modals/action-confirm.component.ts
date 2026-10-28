import { Component } from '@angular/core'
import { i18nPipe } from '@start9labs/shared'
import { TuiButton, TuiDialogContext, TuiNotification } from '@taiga-ui/core'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { PkgIconDirective } from 'src/app/routes/portal/components/pkg-icon'
import { PackageActionData } from './action-input.component'

@Component({
  template: `
    <div class="service-title">
      <img [pkgIcon]="pkgInfo.id" />
      <h4>{{ pkgInfo.title }}</h4>
    </div>
    <div tuiNotification appearance="warning">
      <div [innerHTML]="warning"></div>
    </div>
    <footer class="g-buttons">
      <button tuiButton appearance="flat" (click)="context.completeWith(false)">
        {{ 'Cancel' | i18n }}
      </button>
      <button tuiButton (click)="context.completeWith(true)">
        {{ 'Run' | i18n }}
      </button>
    </footer>
  `,
  styles: `
    .service-title {
      display: inline-flex;
      align-items: center;
      margin-bottom: 1.5rem;

      img {
        height: 1.25rem;
        margin-right: 0.25rem;
        border-radius: 100%;
      }

      h4 {
        margin: 0;
      }
    }

    footer {
      margin-top: 1.5rem;
    }
  `,
  imports: [TuiButton, TuiNotification, i18nPipe, PkgIconDirective],
})
export class ActionConfirmModal {
  readonly context =
    injectContext<TuiDialogContext<boolean, PackageActionData>>()

  readonly pkgInfo = this.context.data.pkgInfo
  readonly warning = this.context.data.actionInfo.metadata.warning
}

export const ACTION_CONFIRM_MODAL = new PolymorpheusComponent(
  ActionConfirmModal,
)
