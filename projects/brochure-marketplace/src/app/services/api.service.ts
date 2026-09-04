import {
  GetPackageRes,
  GetPackagesRes,
  MarketplacePkg,
} from '@start9labs/marketplace'
import { T } from '@start9labs/start-core'

export abstract class ApiService {
  abstract getRegistryInfo(registryUrl: string): Promise<T.RegistryInfo>

  abstract getRegistryPackage(
    registryUrl: string,
    id: string,
    targetVersion: string | null,
  ): Promise<GetPackageRes>

  abstract getRegistryPackages(registryUrl: string): Promise<GetPackagesRes>

  abstract getStaticProxy(
    pkg: MarketplacePkg,
    path: 'LICENSE.md' | 'instructions.md',
  ): Promise<string>
}
