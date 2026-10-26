import { DatePipe } from '@angular/common'
import { Component } from '@angular/core'
import { convertBytes, i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton, TuiDialogContext, TuiTitle } from '@taiga-ui/core'
import { TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'

@Component({
  template: `
    <header tuiHeader>
      <h2 tuiTitle [id]="context.id">
        {{ 'Apply version-history change?' | i18n }}
      </h2>
    </header>
    <p>
      {{
        'This permanently deletes the checkpoints listed in the preview.' | i18n
      }}
    </p>
    @for (change of context.data; track change.packageId) {
      <section>
        <h3 tuiTitle>{{ change.name }}</h3>
        <ul>
          @for (snapshot of change.removed; track snapshot.id) {
            <li>
              {{ snapshot.completedAt | date: 'medium' }} ·
              {{ formatBytes(snapshot.logicalSize) }}
            </li>
          }
        </ul>
      </section>
    }
    <footer class="g-buttons">
      <button
        tuiButton
        type="button"
        appearance="secondary"
        (click)="context.completeWith(false)"
      >
        {{ 'Cancel' | i18n }}
      </button>
      <button
        tuiButton
        type="button"
        appearance="primary-destructive"
        (click)="context.completeWith(true)"
      >
        {{ 'Apply' | i18n }}
      </button>
    </footer>
  `,
  styles: `
    :host {
      display: block;
      overflow-wrap: anywhere;
    }

    ul {
      padding-inline-start: 1.25rem;
    }
  `,
  imports: [DatePipe, i18nPipe, TuiButton, TuiHeader, TuiTitle],
})
class RetentionConfirm {
  protected readonly context =
    injectContext<
      TuiDialogContext<
        boolean,
        { packageId: string; name: string; removed: T.ServiceSnapshot[] }[]
      >
    >()
  protected readonly formatBytes = convertBytes
}

export const BACKUP_RETENTION_CONFIRM = new PolymorpheusComponent(
  RetentionConfirm,
)
