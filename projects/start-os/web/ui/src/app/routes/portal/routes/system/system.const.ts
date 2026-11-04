export const TRUSTED_KEYS_MENU_ITEM = {
  icon: '/assets/icons/key-chain.svg',
  item: 'Trusted Keys',
  link: 'trusted-keys',
} as const

export const TRUSTED_KEYS_LINK = ['/system', TRUSTED_KEYS_MENU_ITEM.link]

export const DOWNLOADS_MENU_ITEM = {
  icon: '@tui.share-2',
  item: 'Download sharing',
  link: 'downloads',
} as const

export const SYSTEM_MENU = [
  [
    {
      icon: '@tui.wrench',
      item: 'General Settings',
      link: 'general',
    },
  ],
  [
    {
      icon: '@tui.copy-plus',
      item: 'Create Backup',
      link: 'backup',
    },
    {
      icon: '@tui.database-backup',
      item: 'Restore Backup',
      link: 'restore',
    },
  ],
  [
    {
      icon: '@tui.monitor',
      item: 'StartOS UI',
      link: 'interfaces',
    },
    {
      icon: '@tui.mail',
      item: 'SMTP',
      link: 'email',
    },
    {
      icon: '@tui.wifi',
      item: 'WiFi',
      link: 'wifi',
    },
  ],
  [
    {
      icon: '@tui.door-open',
      item: 'Gateways',
      link: 'gateways',
    },
    {
      icon: '@tui.globe',
      item: 'Outbound Traffic',
      link: 'outbound-traffic',
    },
  ],
  [
    {
      icon: '@tui.store',
      item: 'Registries',
      link: 'registries',
    },
    DOWNLOADS_MENU_ITEM,
    TRUSTED_KEYS_MENU_ITEM,
  ],
  [
    {
      icon: '@tui.clock',
      item: 'Active Sessions',
      link: 'sessions',
    },
    {
      icon: '@tui.terminal',
      item: 'SSH Access',
      link: 'ssh',
    },
    {
      icon: '@tui.key',
      item: 'Change Password',
      link: 'password',
    },
  ],
  [
    {
      icon: '@tui.scroll-text',
      item: 'OS Logs',
      link: 'os-logs',
    },
    {
      icon: '@tui.scroll-text',
      item: 'Kernel Logs',
      link: 'kernel-logs',
    },
  ],
] as const
