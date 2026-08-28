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

export const registriesManifestPath = '/.well-known/startos/registries.json'

/** Pins for Start9's own registries, standing in while the manifest is unreachable. */
export const defaultIdentities: Record<
  string,
  { name: string; icon: string | null }
> = {
  [knownRegistries.start9]: {
    name: 'Start9 Registry',
    icon: 'assets/img/icon_transparent.png',
  },
  [knownRegistries.community]: {
    name: 'Community Registry',
    icon: 'assets/img/community-icon.png',
  },
  [knownRegistries.start9Alpha]: {
    name: 'Alpha Registry',
    icon: 'assets/img/icon_alpha.png',
  },
  [knownRegistries.start9Beta]: {
    name: 'Beta Registry',
    icon: 'assets/img/icon_beta.png',
  },
  [knownRegistries.communityBeta]: {
    name: 'Community Beta Registry',
    icon: null,
  },
}
