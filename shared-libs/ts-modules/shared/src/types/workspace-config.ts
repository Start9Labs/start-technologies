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

export const knownRegistries = {
  ...defaultRegistries,
  start9Alpha: 'https://alpha-registry-x.start9.com/',
  start9Beta: 'https://beta-registry.start9.com/',
  communityBeta: 'https://community-beta-registry.start9.com/',
} as const
