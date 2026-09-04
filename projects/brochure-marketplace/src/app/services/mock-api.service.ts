import { Injectable } from '@angular/core'
import { ApiService } from './api.service'
import { T } from '@start9labs/start-core'
import { GetPackageRes, GetPackagesRes } from '@start9labs/marketplace'
import { Mock } from './api.fixures'

import markdown from './md-sample.md'

@Injectable()
export class MockApiService extends ApiService {
  constructor() {
    super()
  }

  async getRegistryInfo(): Promise<T.RegistryInfo> {
    await this.pauseFor(1000)
    return Mock.RegistryInfo
  }

  async getRegistryPackage(
    _registryUrl: string,
    id: string,
    _targetVersion: string | null,
  ): Promise<GetPackageRes> {
    await this.pauseFor(1000)
    return Mock.RegistryPackages[id]!
  }

  async getRegistryPackages(): Promise<GetPackagesRes> {
    await this.pauseFor(1000)
    return Mock.RegistryPackages
  }

  async getStaticProxy(): Promise<string> {
    await this.pauseFor(1000)
    return markdown
  }

  private pauseFor(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms))
  }
}
