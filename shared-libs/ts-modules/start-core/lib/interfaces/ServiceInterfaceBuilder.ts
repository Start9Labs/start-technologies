import { ServiceInterfaceType } from '../types'
import { Effects } from '../Effects'
import { Scheme } from './Host'

/**
 * A helper class for creating a Network Interface
 *
 * Network Interfaces are collections of web addresses that expose the same API or other resource,
 * display to the user with under a common name and description.
 *
 * All URIs on an interface inherit the same ui: bool, basic auth credentials, path, and search (query) params
 *
 * @param options
 * @returns
 */
export class ServiceInterfaceBuilder {
  constructor(
    readonly options: {
      effects: Effects
      name: string
      id: string
      description: string
      type: ServiceInterfaceType
      username: string | null
      path: string
      query: Record<string, string>
      schemeOverride: { ssl: Scheme; noSsl: Scheme } | null
      masked: boolean
      preferredLauncherAddress?: string | null
    },
  ) {}
}
