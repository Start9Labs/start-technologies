import { Injectable } from '@angular/core'
import { ApiService, RegistryClient } from './api.service'
import { RPC, createRpcClient, tableTransport } from '@start9labs/start-core'
import { Mock } from './api.fixures'

import markdown from './md-sample.md'

@Injectable()
export class MockApiService extends ApiService {
  private readonly client = createRpcClient<RegistryClient>(
    tableTransport<RPC.Registry>({
      info: async () => {
        await this.pauseFor(1000)
        return Mock.RegistryInfo
      },
      'package.get': async params => {
        await this.pauseFor(1000)
        return params.id
          ? Mock.RegistryPackages[params.id]!
          : Mock.RegistryPackages
      },
    }),
  )

  constructor() {
    super()
  }

  registry(): RegistryClient {
    return this.client
  }

  async getStaticProxy(): Promise<string> {
    await this.pauseFor(1000)
    return markdown
  }

  private pauseFor(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms))
  }
}
