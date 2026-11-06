export const TRUST_PERMISSIONS = [
  {
    context: 'marketplace',
    title: 'Packages',
    label: 'Enabled for service packages',
    enable: 'Trust for packages',
    disable: 'Distrust for packages',
  },
  {
    context: 'os',
    title: 'StartOS updates',
    label: 'Enabled for StartOS updates',
    enable: 'Trust for StartOS updates',
    disable: 'Distrust for StartOS updates',
  },
] as const
