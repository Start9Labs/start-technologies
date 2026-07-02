import { inject, Injectable } from '@angular/core'
import { ApiService, OutboundVpn } from 'src/app/services/api/api.service'
import { FormService } from 'src/app/services/form.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Injectable({ providedIn: 'root' })
export class OutboundService extends FormService<OutboundVpn[]> {
  private readonly api = inject(ApiService)
  private readonly i18n = inject(i18nPipe)

  async load(): Promise<OutboundVpn[]> {
    return this.api.vpnClientList()
  }

  async store(): Promise<void> {
    // List doesn't have a single store operation
  }

  update(
    id: string,
    data: { label: string; target: string; mtu: number | null },
  ) {
    return this.actions.run(
      async () => {
        await this.api.vpnClientUpdate({ id, ...data })
        this.refresh()
      },
      {
        loading: this.i18n.transform('Updating VPN client...'),
        success: this.i18n.transform('VPN client updated'),
        restart: true,
      },
    )
  }

  create(data: { label: string; target: string; config: File }) {
    return this.actions.run(
      async () => {
        const configText = await data.config.text()
        await this.api.vpnClientCreate({
          label: data.label,
          target: data.target,
          config: configText,
        })
        this.refresh()
      },
      {
        loading: this.i18n.transform('Creating VPN client...'),
        success: this.i18n.transform('VPN client created'),
        restart: true,
      },
    )
  }

  remove(id: string) {
    return this.actions.run(
      async () => {
        await this.api.vpnClientDelete({ id })
        this.refresh()
      },
      {
        loading: this.i18n.transform('Removing VPN client...'),
        success: this.i18n.transform('VPN client removed'),
        restart: true,
      },
    )
  }

  setEnabled(id: string, enabled: boolean) {
    return this.actions.run(
      async () => {
        await this.api.vpnClientSetEnabled({ id, enabled })
        this.refresh()
      },
      {
        loading: enabled
          ? this.i18n.transform('Enabling VPN client...')
          : this.i18n.transform('Disabling VPN client...'),
        success: enabled
          ? this.i18n.transform('VPN client enabled')
          : this.i18n.transform('VPN client disabled'),
        restart: true,
      },
    )
  }
}
