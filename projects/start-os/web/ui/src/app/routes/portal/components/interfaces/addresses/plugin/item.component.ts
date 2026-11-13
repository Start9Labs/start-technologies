import { Component, input, signal } from '@angular/core'
import { i18nPipe } from '@start9labs/shared'
import { utils } from '@start9labs/start-core'
import { TuiButton } from '@taiga-ui/core'
import {
  MappedServiceInterface,
  PluginAddress,
  PluginAddressGroup,
} from '../../interface.service'
import { PluginActionsComponent } from './actions.component'

@Component({
  selector: 'tr[pluginAddress]',
  template: `
    <td>
      {{
        address().scheme?.toUpperCase() ??
          (address().hostnameInfo.ssl ? 'SSL' : '—')
      }}
    </td>
    <td>{{ transportLabels[address().transport] }}</td>
    <td class="url-cell">
      <div class="url">
        @if (address().masked && currentlyMasked()) {
          <span>
            ••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••
          </span>
        } @else {
          <span>{{ address().url }}</span>
        }
        @if (address().masked) {
          <button
            tuiIconButton
            appearance="flat-grayscale"
            size="xs"
            [iconStart]="currentlyMasked() ? '@tui.eye' : '@tui.eye-off'"
            (click)="currentlyMasked.set(!currentlyMasked())"
          >
            {{ (currentlyMasked() ? 'Reveal' : 'Hide') | i18n }}
          </button>
        }
      </div>
    </td>
    <td
      pluginActions
      (click.stop)="(0)"
      [address]="address()"
      [pluginGroup]="pluginGroup()"
      [packageId]="packageId()"
      [value]="value()"
      [(currentlyMasked)]="currentlyMasked"
      [style.width.rem]="5"
    ></td>
  `,
  styles: `
    .url {
      display: flex;
      align-items: center;
      gap: 0.25rem;

      span {
        white-space: normal;
        word-break: break-all;
        display: -webkit-box;
        -webkit-box-orient: vertical;
        -webkit-line-clamp: 1;
        overflow: hidden;
      }
    }

    :host-context(tui-root._mobile) {
      .url button {
        display: none;
      }

      grid-template-columns: 1fr auto auto;

      .url-cell {
        grid-area: 2 / 1 / 2 / 3;
      }

      td {
        width: auto !important;
        align-content: center;
      }

      td:last-child {
        grid-area: 1 / 3 / 3 / 4;
        align-self: center;
      }
    }
  `,
  imports: [PluginActionsComponent, TuiButton, i18nPipe],
})
export class PluginItemComponent {
  readonly currentlyMasked = signal(true)
  readonly transportLabels = utils.transportLabels

  readonly address = input.required<PluginAddress>()
  readonly pluginGroup = input.required<PluginAddressGroup>()
  readonly packageId = input('')
  readonly value = input<MappedServiceInterface | undefined>()
}
