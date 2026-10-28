import { inject, Injectable } from '@angular/core'
import { ApiService, ScheduleWindow } from 'src/app/services/api/api.service'
import { FormService } from 'src/app/services/form.service'

@Injectable({ providedIn: 'root' })
export class BlackoutService extends FormService<ScheduleWindow[]> {
  private readonly api = inject(ApiService)

  async load() {
    return this.api.wifiBlackoutGet()
  }

  async store(windows: ScheduleWindow[]) {
    await this.api.wifiBlackoutSet({ windows, confirmImmediateBlackout: true })
  }

  /** Whether the router would turn Wi-Fi off on saving. Applies nothing. */
  async preview(windows: ScheduleWindow[]): Promise<boolean> {
    const result = await this.api.wifiBlackoutSet({
      windows,
      confirmImmediateBlackout: false,
    })
    return result.pendingImmediateBlackout
  }
}
