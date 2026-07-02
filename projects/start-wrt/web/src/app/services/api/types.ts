export type UciSection =
  | NetworkInterfaceSection
  | NetworkDeviceSection
  | DdnsSection
  | DhcpSection
  | DhcpHostSection
  | FirewallRuleSection
  | FirewallRedirectSection

export type NetworkInterfaceSection = {
  type: 'interface'
  name: string | null
  options: {
    proto: 'dhcp' | 'static' | 'pppoe' | 'dhcpv6' | '6rd' | 'none'
    peerdns?: '0' | '1' // 0=use custom DNS, 1=use ISP DNS
    // DHCPv6/SLAAC options
    reqaddress?: 'try' | 'force'
    reqprefix?: 'auto' | string
    // IPv4 options
    ipaddr?: string
    netmask?: string
    gateway?: string
    // PPPoE options
    username?: string
    password?: string
    // Interface/device
    device?: string
    ifname?: string
    // MAC address
    macaddr?: string
    // IPv6 static options
    ip6addr?: string
    ip6gw?: string
    ip6prefix?: string
    ip6ifaceid?: string
    ip6assign?: string // Prefix delegation length
    // 6RD options
    peeraddr?: string
    ip6prefixlen?: string
  }
  lists: {
    dns?: string[] // Custom DNS servers for this interface
  }
}

export type NetworkDeviceSection = {
  type: 'device'
  name: string | null
  options: {
    name?: string
    type?: 'bridge' | string
    macaddr?: string
    mtu?: string
    ports?: string
  }
  lists: {
    ports?: string[]
  }
}

export type DdnsSection = {
  type: 'service'
  name: string | null
  options: {
    enabled?: '0' | '1'
    service_name?: string
    lookup_host?: string
    domain?: string
    username?: string
    password?: string
    ip_source?: 'network' | 'interface' | 'web'
    ip_network?: string
    interface?: string
  }
  lists: {}
}

export type DhcpSection = {
  type: 'dhcp'
  name: string | null
  options: {
    interface?: string
    start?: string
    limit?: string
    leasetime?: string
    // IPv6 options
    ra?: 'disabled' | 'server' | 'relay' | 'hybrid'
    dhcpv6?: 'disabled' | 'server' | 'relay' | 'hybrid'
    ra_management?: '0' | '1' | '2'
    ra_default?: '0' | '1' | '2'
  }
  lists: {}
}

// Static DHCP host reservation
export type DhcpHostSection = {
  type: 'host'
  name: string | null
  options: {
    mac: string // MAC address
    ip?: string // Static IPv4 address
    hostid?: string // Static IPv6 suffix
    name?: string // Custom hostname/label
    dns?: '1' | '0' // Add to local DNS
  }
  lists: {}
}

// Firewall rule for MAC blocking or IPv6 port forwarding
export type FirewallRuleSection = {
  type: 'rule'
  name: string | null
  options: {
    src?: string // e.g., 'lan', 'wan'
    dest?: string // e.g., 'wan', 'lan'
    src_mac?: string // MAC address to block
    target?: 'ACCEPT' | 'REJECT' | 'DROP'
    enabled?: '0' | '1'
    name?: string // Rule description
    // IPv6 port forwarding options
    proto?: 'tcp' | 'udp' | 'tcp udp'
    dest_ip?: string // Destination IP (for IPv6 allow rules)
    dest_port?: string // Destination port
    src_ip?: string // Source IP/CIDR restriction
    family?: 'ipv4' | 'ipv6' // Address family
    // Published port metadata (ignored by firewall, used by our app)
    _pp_id?: string // Links IPv4 redirect and IPv6 rule
    _pp_mac?: string // Device MAC address
  }
  lists: {
    src_mac?: string[] // Can also be a list
  }
}

// Firewall redirect for port forwarding
export type FirewallRedirectSection = {
  type: 'redirect'
  name: string | null
  options: {
    name?: string // Purpose/description
    src?: string // Source zone, e.g., 'wan'
    dest?: string // Destination zone, e.g., 'lan'
    target?: 'DNAT' | 'SNAT'
    proto?: 'tcp' | 'udp' | 'tcp udp'
    src_ip?: string // Source IP/CIDR restriction
    src_dport?: string // External port
    dest_ip?: string // Internal IP address
    dest_port?: string // Internal port
    enabled?: '0' | '1'
    // Published port metadata (ignored by firewall, used by our app)
    _pp_id?: string // Links IPv4 redirect and IPv6 rule
    _pp_mac?: string // Device MAC address
  }
  lists: {}
}

export type UciFile<T extends UciSection> = {
  sections: T[]
  modified: string
}
