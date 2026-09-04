import { T } from '@start9labs/start-core'

import manifest from '../../well-known/startos/registries.json'

export type AccessType =
  | 'tor'
  | 'mdns'
  | 'localhost'
  | 'ipv4'
  | 'ipv6'
  | 'domain'
  | 'wan-ipv4'

export type WorkspaceConfig = {
  gitHash: string
  useMocks: boolean
  // each key corresponds to a project and values adjust settings for that project, eg: ui, setup-wizard
  ui: {
    api: {
      url: string
      version: string
    }
    mocks: {
      maskAs: AccessType
      maskAsHttps: boolean
      skipStartupAlerts: boolean
    }
  }
  defaultRegistry: string
}

export const defaultRegistries = {
  start9: 'https://registry.start9.com/',
  community: 'https://community-registry.start9.com/',
} as const

export const registriesManifestPath = '/.well-known/startos/registries.json'

/** Used when the published list can't be fetched. */
export const registriesSnapshot: T.KnownRegistry[] = manifest.registries
