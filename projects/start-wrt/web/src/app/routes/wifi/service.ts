import { inject, Injectable } from '@angular/core'
import {
  AffectedPublishedPort,
  ApiService,
  WifiConfig,
  WifiPassword,
} from 'src/app/services/api/api.service'
import { FormService } from 'src/app/services/form.service'
import { isNetworkError } from 'src/app/services/connection.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

export type { WifiConfig, WifiPassword }

@Injectable()
export class WifiService extends FormService<WifiConfig> {
  private readonly api = inject(ApiService)
  private readonly i18n = inject(i18nPipe)

  async load() {
    return this.api.wifiGet()
  }

  async store(data: WifiConfig) {
    // Real apply — confirm any published-port cleanup (the caller previews and
    // confirms first where a reassignment can break published ports).
    await this.api.wifiSet({ ...data, confirmPublishedPortDeletion: true })
  }

  /**
   * Dry-run: returns the published ports that would be deleted if this config
   * were applied (i.e. devices whose WiFi profile is being vacated). Applies
   * nothing.
   */
  async previewWifi(data: WifiConfig): Promise<AffectedPublishedPort[]> {
    const result = await this.api.wifiSet({
      ...data,
      confirmPublishedPortDeletion: false,
    })
    return result.pendingPublishedPortDeletions
  }

  async addPassword(password: WifiPassword) {
    await this.actions.run(async () => {
      const current = this.data()
      if (!current) return
      await this.api.wifiSet({
        ...current,
        passwords: [...current.passwords, password],
        confirmPublishedPortDeletion: true,
      })
      this.refresh()
    })
  }

  async saveWithRestart(data: WifiConfig): Promise<boolean> {
    return this.actions.run(() => this.store(data), {
      loading: this.i18n.transform('Restarting WiFi...'),
      success: this.i18n.transform('WiFi settings saved'),
      restart: true,
    })
  }

  saveForSsidChange(data: WifiConfig): Promise<boolean> {
    // The ReconnectDialog owns the reconnect UX (with the explicit "reconnect
    // to <ssid>" instruction). Suppress the global indicator BEFORE the restart
    // drops the connection so it doesn't also pop a generic toast.
    //
    // Resolve `true` when the save lands, and `false` only for the EXPECTED
    // network-drop rejection (the SSID restart tearing down our own connection)
    // — the dialog recovers that via its reconnect poll. A non-network rejection
    // is a real backend failure (bad config, UCI conflict, restart failed): do
    // NOT discard it, throw so the dialog can close and surface it instead of
    // spinning forever.
    this.connection.suppress()
    return this.api
      .wifiSet({ ...data, confirmPublishedPortDeletion: true })
      .then(
        () => true,
        e => {
          if (isNetworkError(e)) return false
          throw e
        },
      )
  }

  async deletePassword(index: number) {
    await this.actions.run(async () => {
      const current = this.data()
      if (!current) return
      const passwords = current.passwords.filter((_, i) => i !== index)
      await this.api.wifiSet({
        ...current,
        passwords,
        confirmPublishedPortDeletion: true,
      })
      this.refresh()
    })
  }
}
