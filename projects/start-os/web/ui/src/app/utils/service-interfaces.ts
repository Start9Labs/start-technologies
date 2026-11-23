import { byIndex } from '@start9labs/shared'
import { T } from '@start9labs/start-core'

const byInterfaceIndex = byIndex<{ id: string; index?: number | null }>(
  iface => iface.id,
)

/** A package's single-port service interfaces, in the package's interface order. */
export function packageInterfaces(
  hosts: Record<string, T.Host>,
): T.ServiceInterface[] {
  return Object.values(hosts)
    .flatMap(host => Object.values(host.bindings))
    .flatMap(binding => Object.values(binding.interfaces))
    .sort(byInterfaceIndex)
}

/** A host's interface names, single-port then ranges, each in the package's order. */
export function hostInterfaceNames(host: T.Host): string[] {
  return [
    ...packageInterfaces({ host }).map(iface => iface.name),
    ...Object.values(host.bindingRanges)
      .flatMap(range => (range.interface ? [range.interface] : []))
      .sort(byInterfaceIndex)
      .map(iface => iface.name),
  ]
}
