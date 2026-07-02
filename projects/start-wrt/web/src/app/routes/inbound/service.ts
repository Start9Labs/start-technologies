import { inject, Injectable } from '@angular/core'
import {
  ApiService,
  VpnServer,
  VpnServerConfig,
  VpnServerPeer,
  VpnServerPeerAddResponse,
} from 'src/app/services/api/api.service'
import { FormService } from 'src/app/services/form.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

export type {
  VpnServer,
  VpnServerPeer,
  VpnServerConfig,
  VpnServerPeerAddResponse,
}

@Injectable()
export class InboundService extends FormService<VpnServer[]> {
  private readonly api = inject(ApiService)
  private readonly i18n = inject(i18nPipe)

  async load() {
    const res = await this.api.vpnServerList()
    return res.servers
  }

  async store() {}

  async setServer(profile: string, config: VpnServerConfig) {
    await this.actions.run(
      async () => {
        await this.api.vpnServerSet({ profile, config })
        this.refresh()
      },
      {
        loading: this.i18n.transform('Applying VPN server settings...'),
        success: this.i18n.transform('VPN server settings applied'),
        restart: true,
      },
    )
  }

  async deleteServer(profile: string) {
    await this.actions.run(
      async () => {
        await this.api.vpnServerDelete({ profile })
        this.refresh()
      },
      {
        loading: this.i18n.transform('Removing VPN server...'),
        success: this.i18n.transform('VPN server removed'),
        restart: true,
      },
    )
  }

  async addPeer(
    profile: string,
    peer: VpnServerPeer,
  ): Promise<VpnServerPeerAddResponse | undefined> {
    let response: VpnServerPeerAddResponse | undefined
    await this.actions.run(
      async () => {
        response = await this.api.vpnServerPeerAdd({ profile, peer })
        this.refresh()
      },
      {
        loading: this.i18n.transform('Adding VPN peer...'),
        success: this.i18n.transform('VPN peer added'),
        restart: true,
      },
    )
    return response
  }

  async deletePeer(profile: string, public_key: string) {
    await this.actions.run(
      async () => {
        await this.api.vpnServerPeerDelete({ profile, public_key })
        this.refresh()
      },
      {
        loading: this.i18n.transform('Removing VPN peer...'),
        success: this.i18n.transform('VPN peer removed'),
        restart: true,
      },
    )
  }
}
