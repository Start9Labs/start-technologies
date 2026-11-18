import { Component } from '@angular/core'
import { DocsLinkDirective, i18nKey, i18nPipe } from '@start9labs/shared'
import {
  TuiButton,
  TuiDialogContext,
  TuiIcon,
  TuiLink,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiCardLarge, TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { NutMode } from './form'

const MODES: readonly {
  mode: NutMode
  icon: string
  name: i18nKey
  description: i18nKey
  fragment: string
}[] = [
  {
    mode: 'server',
    icon: '@tui.usb',
    name: 'NUT server',
    description:
      'The UPS’s USB or serial cable is connected to this server. StartOS reads the UPS directly and can share it with other devices on your network.',
    fragment: '#nut-server',
  },
  {
    mode: 'client',
    icon: '@tui.network',
    name: 'NUT client',
    description:
      'The UPS’s USB or serial cable is connected to another device running a NUT server, such as a NAS or router. You will need its address and a monitor account on it.',
    fragment: '#nut-client',
  },
]

@Component({
  template: `
    @for (option of modes; track option.mode) {
      <section tuiCardLarge="compact" appearance="secondary-grayscale">
        <header tuiHeader="body-l">
          <tui-icon [icon]="option.icon" />
          <h3 tuiTitle>{{ option.name | i18n }}</h3>
        </header>
        <div>{{ option.description | i18n }}</div>

        <footer>
          <a
            tuiLink
            appearance=""
            iconEnd="@tui.book-open-text"
            docsLink
            path="/start-os/ups-monitor.html"
            [fragment]="option.fragment"
            [textContent]="'Learn more' | i18n"
          ></a>
          <button
            tuiButton
            size="m"
            (click)="context.completeWith(option.mode)"
          >
            {{ 'Connect' | i18n }}
          </button>
        </footer>
      </section>
    }
  `,
  styles: `
    :host {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 1rem;

      :host-context(tui-root._mobile) & {
        grid-template-columns: 1fr;
      }
    }

    tui-icon {
      font-size: 1.75rem;
    }

    [tuiCardLarge] {
      box-shadow: inset 0 0 0 1px var(--tui-border-normal);
      font-weight: 600;
    }

    [tuiHeader] {
      font-weight: bold;
      gap: 0.75rem;
    }

    footer {
      display: flex;
      align-items: center;
      flex-wrap: wrap;
      justify-content: space-between;
      gap: 0.75rem;
      margin-block-start: auto;
    }
  `,
  imports: [
    DocsLinkDirective,
    TuiButton,
    TuiIcon,
    TuiLink,
    i18nPipe,
    TuiCardLarge,
    TuiHeader,
    TuiTitle,
  ],
})
class NutModePicker {
  protected readonly context = injectContext<TuiDialogContext<NutMode>>()
  protected readonly modes = MODES
}

export const NUT_MODE_PICKER = new PolymorpheusComponent(NutModePicker)
