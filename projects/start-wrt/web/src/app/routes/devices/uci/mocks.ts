import { DhcpHostSection } from 'src/app/services/api/types'

// Mock DHCP hosts - known devices with static reservations or custom names
export const mockDhcpHosts: DhcpHostSection[] = [
  {
    type: 'host',
    name: 'host_humble_weeds',
    options: {
      mac: '00:1A:2B:3C:4D:5E',
      ip: '192.168.1.100',
      hostid: '1::100',
      name: 'humble-weeds',
      dns: '1',
    },
    lists: {},
  },
  {
    type: 'host',
    name: 'host_pixel',
    options: {
      mac: '00:1A:2B:3C:4D:5F',
      ip: '192.168.1.101', // IPv4 reservation for published port
      name: 'Pixel',
      dns: '1',
    },
    lists: {},
  },
  {
    type: 'host',
    name: 'host_iphone',
    options: {
      mac: 'DE:AD:BE:EF:CA:FE',
      name: 'iPhone 13',
      dns: '1',
    },
    lists: {},
  },
  {
    type: 'host',
    name: 'host_mariusz',
    options: {
      mac: 'DE:AD:BE:EF:CA:FF',
      name: "Mariusz's Phone",
      dns: '1',
    },
    lists: {},
  },
  {
    type: 'host',
    name: 'host_macbook',
    options: {
      mac: 'AA:BB:CC:DD:EE:01',
      name: 'MacBook Pro',
      dns: '1',
    },
    lists: {},
  },
]

// Mock ARP table output (from `ip neigh show`)
// Format: IP dev INTERFACE lladdr MAC STATE

// Base ARP output (IPv4 + link-local IPv6)
const mockArpBase = `192.168.1.100 dev br-lan lladdr 00:1a:2b:3c:4d:5e REACHABLE
192.168.1.101 dev br-lan lladdr 00:1a:2b:3c:4d:5f STALE
192.168.1.102 dev br-lan lladdr de:ad:be:ef:ca:fe STALE
192.168.1.103 dev br-lan lladdr de:ad:be:ef:ca:ff REACHABLE
fe80::1a:2bff:fe3c:4d5e dev br-lan lladdr 00:1a:2b:3c:4d:5e REACHABLE
fe80::1a:2bff:fe3c:4d5f dev br-lan lladdr 00:1a:2b:3c:4d:5f STALE`

// Global IPv6 addresses (when WAN IPv6 is enabled)
const mockArpGlobalIpv6 = `2001:db8:abcd:1::100 dev br-lan lladdr 00:1a:2b:3c:4d:5e REACHABLE
2001:db8:abcd:1::101 dev br-lan lladdr 00:1a:2b:3c:4d:5f STALE
2001:db8:abcd:1::102 dev br-lan lladdr de:ad:be:ef:ca:fe STALE
2001:db8:abcd:1::103 dev br-lan lladdr de:ad:be:ef:ca:ff REACHABLE`

// ULA IPv6 addresses (when WAN IPv6 is disabled but LAN IPv6 is enabled)
const mockArpUlaIpv6 = `fd00:1234:5678::100 dev br-lan lladdr 00:1a:2b:3c:4d:5e REACHABLE
fd00:1234:5678::101 dev br-lan lladdr 00:1a:2b:3c:4d:5f STALE
fd00:1234:5678::102 dev br-lan lladdr de:ad:be:ef:ca:fe STALE
fd00:1234:5678::103 dev br-lan lladdr de:ad:be:ef:ca:ff REACHABLE`

// Export function to get ARP output based on IPv6 status
export function getMockArpOutput(
  wanIpv6Enabled: boolean,
  lanIpv6Enabled: boolean,
): string {
  if (!lanIpv6Enabled) {
    // No IPv6 at all - just base (IPv4 + link-local)
    return mockArpBase
  }
  if (wanIpv6Enabled) {
    // Global IPv6 addresses
    return `${mockArpBase}
${mockArpGlobalIpv6}`
  }
  // ULA IPv6 addresses (LAN IPv6 only)
  return `${mockArpBase}
${mockArpUlaIpv6}`
}

// Default export for backwards compatibility (assumes both enabled)
export const mockArpOutput = `${mockArpBase}
${mockArpGlobalIpv6}`

// Mock DHCP leases file output (from /tmp/dhcp.leases)
// Format: expiry_timestamp mac_address ip_address hostname client_id
export const mockDhcpLeasesOutput = `1736899200 00:1a:2b:3c:4d:5e 192.168.1.100 humble-weeds 01:00:1a:2b:3c:4d:5e
1736899200 00:1a:2b:3c:4d:5f 192.168.1.101 Pixel 01:00:1a:2b:3c:4d:5f
1736812800 de:ad:be:ef:ca:fe 192.168.1.102 iPhone13 01:de:ad:be:ef:ca:fe
1736812800 de:ad:be:ef:ca:ff 192.168.1.103 android-abc123 01:de:ad:be:ef:ca:ff`

// Mock IPv6 leases (from /tmp/hosts/odhcpd)
// Format: # hostname ip duid iaid name
export const mockDhcp6LeasesOutput = `# humble-weeds fe80::1a:2bff:fe3c:4d5e 00010001 0 humble-weeds
# Pixel fe80::1a:2bff:fe3c:4d5f 00010002 0 Pixel
# iPhone13 fe80::dead:beef:cafe:0001 00010003 0 iPhone13
# android-abc123 fe80::dead:beef:cafe:0002 00010004 0 android-abc123`

// Mock device definitions — IPs are computed dynamically from profile gateways
export interface MockDeviceDef {
  mac: string
  hostname: string
  hostOctet: number // last octet for IPv4, used for IPv6 too
  profileInterface: string // which profile this device belongs to
  connection: string
  status: 'online' | 'offline'
  speed: { up: number; down: number } | null
  dataUsage: number
}

export const MOCK_DEVICE_DEFS: MockDeviceDef[] = [
  {
    mac: '00:1A:2B:3C:4D:5E',
    hostname: 'humble-weeds',
    hostOctet: 100,
    profileInterface: 'lan',
    connection: 'Ethernet',
    status: 'online',
    speed: { up: 2.3, down: 45.7 },
    dataUsage: 156.3,
  },
  {
    mac: '00:1A:2B:3C:4D:5F',
    hostname: 'Pixel',
    hostOctet: 101,
    profileInterface: 'lan',
    connection: 'Wi-Fi 5GHz',
    status: 'online',
    speed: { up: 0.8, down: 12.4 },
    dataUsage: 42.8,
  },
  {
    mac: 'DE:AD:BE:EF:CA:FE',
    hostname: 'iPhone13',
    hostOctet: 102,
    profileInterface: 'lan',
    connection: 'Wi-Fi 2.4GHz',
    status: 'online',
    speed: null,
    dataUsage: 18.2,
  },
  {
    mac: 'DE:AD:BE:EF:CA:FF',
    hostname: 'android-abc123',
    hostOctet: 103,
    profileInterface: 'lan',
    connection: 'Wi-Fi 5GHz',
    status: 'online',
    speed: { up: 0.1, down: 3.2 },
    dataUsage: 87.5,
  },
  {
    mac: 'AA:BB:CC:DD:EE:01',
    hostname: 'macbook-pro',
    hostOctet: 110,
    profileInterface: 'lan',
    connection: 'Wi-Fi 5GHz',
    status: 'offline',
    speed: null,
    dataUsage: 234.1,
  },
  {
    // No DHCP name advertised ('*') and no static host entry — exercises the
    // `device-<mac>` placeholder rendering.
    mac: 'AA:BB:CC:DD:EE:02',
    hostname: '*',
    hostOctet: 111,
    profileInterface: 'lan',
    connection: 'Ethernet',
    status: 'online',
    speed: { up: 0.5, down: 5.0 },
    dataUsage: 12.0,
  },
]

// Mock nlbwmon data generator
// Generates realistic-looking bandwidth data for a given period
export function generateMockDataUsage(
  mac: string,
  period: 'week' | 'month' | '3months',
): { timestamp: number; upload: number; download: number }[] {
  const now = Math.floor(Date.now() / 1000)
  const points: { timestamp: number; upload: number; download: number }[] = []

  // One point per day to match real backend granularity (nlbwmon archives are daily).
  const interval = 24 * 60 * 60
  const numPoints = period === 'week' ? 7 : period === 'month' ? 30 : 90
  const startTime = now - numPoints * interval

  // Generate some variation based on MAC address
  const seed = mac.split(':').reduce((acc, hex) => acc + parseInt(hex, 16), 0)

  for (let i = 0; i < numPoints; i++) {
    const timestamp = startTime + i * interval

    // Generate pseudo-random but consistent data based on MAC and timestamp
    const variation = Math.sin(seed + i * 0.5) * 0.5 + 0.5
    const timeOfDay = (timestamp % (24 * 60 * 60)) / (24 * 60 * 60)
    const activityFactor = timeOfDay > 0.3 && timeOfDay < 0.9 ? 1.5 : 0.5

    // Download is typically higher than upload (in GB scale)
    const baseDownload = 2 * 1024 * 1024 * 1024 * activityFactor * variation // ~2GB base
    const baseUpload = 0.5 * 1024 * 1024 * 1024 * activityFactor * variation // ~500MB base

    points.push({
      timestamp,
      upload: Math.floor(baseUpload * (0.5 + Math.random())),
      download: Math.floor(baseDownload * (0.5 + Math.random())),
    })
  }

  return points
}
