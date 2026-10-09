import { i18nKey } from '@start9labs/shared'

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
      icon: '@tui.save',
      item: 'Backups',
      link: 'backups',
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
    // {
    //   icon: '@tui.award',
    //   item: 'Certificate Authorities',
    //   link: 'authorities',
    // },
    {
      icon: '@tui.globe',
      item: 'DNS' as i18nKey,
      link: 'dns',
    },
  ],
  [
    {
      icon: '@tui.clock',
      item: 'Active Sessions',
      link: 'sessions',
    },
    {
      icon: '@tui.terminal',
      item: 'SSH Keys',
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
