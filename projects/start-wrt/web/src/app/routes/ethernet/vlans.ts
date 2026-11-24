import { Component, inject } from '@angular/core'
import { TuiDialogContext, TuiTitle } from '@taiga-ui/core'
import { TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { fill } from 'src/app/i18n/validation-errors'
import type { ProfileId } from 'src/app/services/api/api.service'

@Component({
  template: `
    <header tuiHeader>
      <hgroup tuiTitle>
        <h2 [id]="context.id">{{ 'VLAN IDs' | i18n }}</h2>
        <p>{{ subtitle }}</p>
      </hgroup>
    </header>
    <table>
      <thead>
        <tr>
          <th>{{ 'VLAN ID' | i18n }}</th>
          <th>{{ 'Security Profile' | i18n }}</th>
        </tr>
      </thead>
      <tbody>
        @for (profile of profiles; track profile.vlan_tag) {
          <tr>
            <td>{{ profile.vlan_tag }}</td>
            <td>{{ profile.fullname }}</td>
          </tr>
        }
      </tbody>
    </table>
  `,
  styles: `
    table {
      width: 100%;
      border-collapse: collapse;
    }

    th,
    td {
      padding: 0.5rem 0;
      text-align: start;
    }

    th {
      font: var(--tui-typography-body-s);
      color: var(--tui-text-secondary);
    }

    tbody tr {
      box-shadow: inset 0 1px var(--tui-border-normal);
    }
  `,
  imports: [TuiHeader, TuiTitle, i18nPipe],
})
export class VlansDialog {
  protected readonly context =
    injectContext<
      TuiDialogContext<void, { port: string; profiles: ProfileId[] }>
    >()

  protected readonly profiles = [...this.context.data.profiles].sort(
    (a, b) => a.vlan_tag - b.vlan_tag,
  )

  protected readonly subtitle = fill(
    inject(i18nPipe).transform(
      "{port} sends each security profile to the switch tagged with its VLAN ID. Set up the switch's VLANs with these IDs.",
    ),
    { port: this.context.data.port },
  )
}

export const VLANS_DIALOG = new PolymorpheusComponent(VlansDialog)
