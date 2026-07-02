import {
  DhcpSection,
  NetworkInterfaceSection,
} from 'src/app/services/api/types'

// LAN interface with IPv6 enabled (SLAAC)
export const lanIpv6Slaac: NetworkInterfaceSection = {
  type: 'interface',
  name: 'lan',
  options: {
    proto: 'static',
    ipaddr: '192.168.0.1',
    netmask: '255.255.255.0',
    ip6assign: '64',
  },
  lists: {},
}

// DHCP section with SLAAC only (ra=server, dhcpv6=disabled)
export const dhcpLanSlaacOnly: DhcpSection = {
  type: 'dhcp',
  name: 'lan',
  options: {
    interface: 'lan',
    start: '100',
    limit: '150',
    leasetime: '12h',
    ra: 'server',
    dhcpv6: 'disabled',
  },
  lists: {},
}

// DHCP section with SLAAC + DHCPv6 (ra=server, dhcpv6=server)
export const dhcpLanSlaacDhcpv6: DhcpSection = {
  type: 'dhcp',
  name: 'lan',
  options: {
    interface: 'lan',
    start: '100',
    limit: '150',
    leasetime: '12h',
    ra: 'server',
    dhcpv6: 'server',
  },
  lists: {},
}

// DHCP section with IPv6 disabled (ra=disabled, dhcpv6=disabled)
export const dhcpLanIpv6Disabled: DhcpSection = {
  type: 'dhcp',
  name: 'lan',
  options: {
    interface: 'lan',
    start: '100',
    limit: '150',
    leasetime: '12h',
    ra: 'disabled',
    dhcpv6: 'disabled',
  },
  lists: {},
}
