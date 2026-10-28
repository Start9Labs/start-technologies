import { i18nKey, i18nPipe } from '@start9labs/shared'
import { ISB, utils } from '@start9labs/start-core'

export function dnsSelectionSpec(
  i18n: i18nPipe,
  automaticName: i18nKey = 'Use Gateway DNS',
) {
  return ISB.Value.union({
    name: i18n.transform('DNS'),
    default: 'automatic',
    variants: ISB.Variants.of({
      automatic: {
        name: i18n.transform(automaticName),
        spec: ISB.InputSpec.of({}),
      },
      static: {
        name: i18n.transform('Static'),
        spec: ISB.InputSpec.of({
          servers: ISB.Value.list(
            ISB.List.text(
              {
                name: i18n.transform('Servers'),
                minLength: 1,
                maxLength: 3,
              },
              {
                placeholder: '1.1.1.1',
                patterns: [
                  {
                    regex: `^(${utils.regexes.ipv4.contains()}(:\\d{1,5})?|${utils.regexes.ipv6.contains()}|\\[${utils.regexes.ipv6.contains()}\\](:\\d{1,5})?)$`,
                    description: i18n.transform(
                      'Must be a valid IPv4 or Ipv6 address with optional port',
                    ),
                  },
                ],
              },
            ),
          ),
        }),
      },
    }),
  })
}

type DnsSelection = ReturnType<typeof dnsSelectionSpec>['_TYPE']

export function dnsSelectionValue(servers: string[] | null): DnsSelection {
  return servers
    ? { selection: 'static', value: { servers } }
    : { selection: 'automatic', value: {} }
}

export function dnsSelectionServers(dns: DnsSelection): string[] | null {
  return dns.selection === 'static' ? dns.value.servers : null
}
