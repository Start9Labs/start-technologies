import { inject, Injectable } from '@angular/core'
import { FormService } from 'src/app/services/form.service'
import { ApiService, DeviceFromApi } from 'src/app/services/api/api.service'
import {
  DataUsagePeriod,
  DataUsagePoint,
  Device,
  DeviceUpdateData,
} from './utils'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

/**
 * Root-provided service for device API calls.
 * Used by DevicesService (FormService) and external routes
 * (published-ports, inbound) that need device data.
 */
@Injectable({
  providedIn: 'root',
})
export class DevicesApiService {
  private readonly api = inject(ApiService)

  async get(): Promise<Device[]> {
    const devices = await this.api.devicesList()
    return devices.map(d => this.mapDevice(d))
  }

  async update(mac: string, data: DeviceUpdateData): Promise<void> {
    await this.api.devicesUpdate({
      mac,
      name: data.name,
      ipv4_static: data.ipv4Static,
      ipv4: data.ipv4,
      ipv6_static: data.ipv6Static,
      ipv6: data.ipv6,
    })
  }

  async forget(mac: string): Promise<void> {
    await this.api.devicesForget({ mac })
  }

  async getDataUsage(
    mac: string,
    period: DataUsagePeriod,
  ): Promise<DataUsagePoint[]> {
    return this.api.devicesDataUsage({ mac, period })
  }

  private mapDevice(d: DeviceFromApi): Device {
    return {
      mac: d.mac,
      name: d.name, // fully resolved server-side
      hostname: d.hostname || '',
      status: d.status,
      connection: d.connection || undefined,
      ipv4: d.ipv4 || undefined,
      ipv6: d.ipv6 || undefined,
      ipv4Static: d.ipv4_static,
      ipv6Static: d.ipv6_static,
      securityProfile: d.security_profile || undefined,
      speed: d.speed || undefined,
      dataUsage: d.data_usage ?? undefined,
    }
  }
}

@Injectable()
export class DevicesService extends FormService<Device[]> {
  private readonly devicesApi = inject(DevicesApiService)
  private readonly i18n = inject(i18nPipe)

  async load(): Promise<Device[]> {
    return this.devicesApi.get()
  }

  async store(): Promise<void> {
    // List doesn't have a single store operation
  }

  // Update device settings
  update(mac: string, data: DeviceUpdateData) {
    return this.actions.run(
      async () => {
        await this.devicesApi.update(mac, data)
        await this.refreshAndWait()
      },
      {
        loading: this.i18n.transform('Updating device'),
        success: this.i18n.transform('Device updated'),
      },
    )
  }

  // Forget device - remove entirely
  forget(mac: string) {
    return this.actions.run(
      async () => {
        await this.devicesApi.forget(mac)
        await this.refreshAndWait()
      },
      {
        loading: this.i18n.transform('Forgetting device'),
        success: this.i18n.transform('Device forgotten'),
      },
    )
  }

  // Get historical data usage for a device
  async getDataUsage(
    mac: string,
    period: DataUsagePeriod,
  ): Promise<DataUsagePoint[]> {
    return this.devicesApi.getDataUsage(mac, period)
  }
}
