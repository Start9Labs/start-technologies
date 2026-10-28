import { MarketplacePkg } from '@start9labs/marketplace'
import { RPC, RpcClient } from '@start9labs/start-core'

export type RegistryClient = RpcClient<RPC.Registry>

export abstract class ApiService {
  /** The registry's RPC methods, as generated from its bindings. */
  abstract registry(url: string): RegistryClient

  abstract getStaticProxy(
    pkg: MarketplacePkg,
    path: 'LICENSE.md' | 'instructions.md',
  ): Promise<string>
}
