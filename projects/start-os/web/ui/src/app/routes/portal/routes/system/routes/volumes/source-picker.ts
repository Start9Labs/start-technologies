import { Component } from '@angular/core'
import { DocsLinkDirective, i18nKey, i18nPipe } from '@start9labs/shared'
import {
  TuiButton,
  TuiDialogContext,
  TuiIcon,
  TuiLink,
  TuiNotification,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiCardLarge, TuiHeader, TuiList } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'

export type VolumeSourceKind = 'local' | 'disk' | 'cifs'

export interface SourcePickerData {
  intro: i18nKey
  docs: string
  sources: readonly {
    kind: VolumeSourceKind
    icon: string
    title: i18nKey
    description: i18nKey
    points: readonly i18nKey[]
  }[]
}

export const VOLUME_SOURCES: SourcePickerData = {
  intro:
    'Shared volumes are only for data that several services, or other devices, need to use. Each service already stores its own data, so most services never need one.',
  docs: '/start-os/shared-volumes.html#create-and-manage-volumes',
  sources: [
    {
      kind: 'local',
      icon: '@tui.folder',
      title: 'Local folder',
      description: 'A new folder on this server’s data drive.',
      points: ['Easiest to set up', 'Deleted when you remove the volume'],
    },
    {
      kind: 'disk',
      icon: '@tui.hard-drive',
      title: 'External drive',
      description:
        'A new or existing folder on a drive connected to this server.',
      points: [
        'Good for large media libraries',
        'Kept when you remove the volume',
      ],
    },
    {
      kind: 'cifs',
      icon: '@tui.network',
      title: 'CIFS share',
      description: 'An existing shared folder on a NAS or another computer.',
      points: ['Use files already on a NAS', 'Kept when you remove the volume'],
    },
  ],
}

@Component({
  template: `
    <p tuiNotification appearance="info" class="intro">
      {{ context.data.intro | i18n }}
    </p>
    @for (source of context.data.sources; track source.kind) {
      <section tuiCardLarge="compact" appearance="secondary-grayscale">
        <header tuiHeader="body-l">
          <tui-icon [icon]="source.icon" />
          <h3 tuiTitle>{{ source.title | i18n }}</h3>
        </header>
        <div>{{ source.description | i18n }}</div>
        <ul tuiList="s">
          @for (point of source.points; track point) {
            <li>{{ point | i18n }}</li>
          }
        </ul>
        <footer>
          <a
            tuiLink
            appearance=""
            iconEnd="@tui.book-open-text"
            docsLink
            [path]="context.data.docs"
            [textContent]="'Learn more' | i18n"
          ></a>
          <button
            tuiButton
            size="m"
            (click)="context.completeWith(source.kind)"
          >
            {{ 'Select' | i18n }}
          </button>
        </footer>
      </section>
    }
  `,
  styles: `
    :host {
      display: grid;
      grid-template-columns: repeat(3, 1fr);
      gap: 1rem;
    }

    .intro {
      grid-column: 1 / -1;
      margin: 0;
    }

    tui-icon {
      font-size: 1.75rem;
    }

    [tuiCardLarge] {
      display: grid;
      grid-row: span 4;
      grid-template-rows: subgrid;
      row-gap: 1.25rem;
      box-shadow: inset 0 0 0 1px var(--tui-border-normal);
      font-weight: 600;
    }

    [tuiHeader] {
      font-weight: bold;
      gap: 0.75rem;
    }

    ul {
      margin-block: 0;
      color: var(--tui-text-secondary);
    }

    footer {
      display: flex;
      align-items: center;
      flex-wrap: wrap;
      justify-content: space-between;
      gap: 0.75rem;
    }

    footer button {
      margin-inline-start: auto;
    }

    :host-context(tui-root._mobile) {
      grid-template-columns: 1fr;
    }
  `,
  imports: [
    DocsLinkDirective,
    TuiButton,
    TuiIcon,
    TuiLink,
    TuiNotification,
    i18nPipe,
    TuiCardLarge,
    TuiHeader,
    TuiTitle,
    TuiList,
  ],
})
class VolumeSourcePicker {
  protected readonly context =
    injectContext<TuiDialogContext<VolumeSourceKind, SourcePickerData>>()
}

export const VOLUME_SOURCE_PICKER = new PolymorpheusComponent(
  VolumeSourcePicker,
)
