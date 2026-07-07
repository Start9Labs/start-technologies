/**
 * Represents an IPv4 or IPv6 address as raw octets with arithmetic and comparison operations.
 *
 * IPv4 addresses have 4 octets, IPv6 addresses have 16 octets.
 *
 * @example
 * ```ts
 * const ip = IpAddress.parse("192.168.1.1")
 * const next = ip.add(1) // 192.168.1.2
 * ```
 */
export class IpAddress {
  private renderedOctets: number[]
  protected constructor(
    public octets: number[],
    private renderedAddress: string,
  ) {
    this.renderedOctets = [...octets]
  }
  /**
   * Parses an IP address string into an IpAddress instance.
   * Supports both IPv4 dotted-decimal and IPv6 colon-hex notation (including `::` shorthand).
   * @param address - The IP address string to parse
   * @returns A new IpAddress instance
   * @throws Error if the address format is invalid
   */
  static parse(address: string): IpAddress {
    let octets
    if (address.includes(':')) {
      octets = new Array(16).fill(0)
      const segs = address.split(':')
      let idx = 0
      let octIdx = 0
      while (segs[idx]) {
        const num = parseInt(segs[idx], 16)
        octets[octIdx++] = num >> 8
        octets[octIdx++] = num & 255
        idx += 1
      }
      const lastSegIdx = segs.length - 1
      if (idx < lastSegIdx) {
        idx = lastSegIdx
        octIdx = 15
        while (segs[idx]) {
          const num = parseInt(segs[idx], 16)
          octets[octIdx--] = num & 255
          octets[octIdx--] = num >> 8
          idx -= 1
        }
      }
    } else {
      octets = address.split('.').map(Number)
      if (octets.length !== 4) throw new Error('invalid ipv4 address')
    }
    if (octets.some(o => isNaN(o) || o > 255)) {
      throw new Error('invalid ip address')
    }
    return new IpAddress(octets, address)
  }
  /**
   * Creates an IpAddress from a raw octet array.
   * @param octets - Array of 4 octets (IPv4) or 16 octets (IPv6), each 0-255
   * @returns A new IpAddress instance
   * @throws Error if the octet array length is not 4 or 16, or any octet exceeds 255
   */
  static fromOctets(octets: number[]) {
    if (octets.length == 4) {
      if (octets.some(o => o > 255)) {
        throw new Error('invalid ip address')
      }
      return new IpAddress(octets, octets.join('.'))
    } else if (octets.length == 16) {
      if (octets.some(o => o > 255)) {
        throw new Error('invalid ip address')
      }
      return new IpAddress(octets, renderIpv6(octets))
    } else {
      throw new Error('invalid ip address')
    }
  }
  /** Returns true if this is an IPv4 address (4 octets). */
  isIpv4(): boolean {
    return this.octets.length === 4
  }
  /** Returns true if this is an IPv6 address (16 octets). */
  isIpv6(): boolean {
    return this.octets.length === 16
  }
  /** Returns true if this is a public IPv4 address (not in any private range). */
  isPublic(): boolean {
    return this.isIpv4() && !PRIVATE_IPV4_RANGES.some(r => r.contains(this))
  }
  /**
   * Returns true if this is an IPv6 global-unicast address (a GUA) — i.e. IPv6
   * that is not loopback, unique-local (`fc00::/7`), or link-local (`fe80::/10`).
   * Mirrors the backend's `ipv6_is_local`, so it identifies the same globally
   * routable address StartOS advertises for DualStack.
   */
  isGua(): boolean {
    return (
      this.isIpv6() &&
      !IPV6_LOOPBACK.contains(this) &&
      !IPV6_ULA.contains(this) &&
      !IPV6_LINK_LOCAL.contains(this)
    )
  }
  /**
   * Returns a new IpAddress incremented by `n`.
   * @param n - The integer amount to add (fractional part is truncated)
   * @returns A new IpAddress with the result
   * @throws Error on overflow
   */
  add(n: number): IpAddress {
    let octets = [...this.octets]
    n = Math.floor(n)
    for (let i = octets.length - 1; i >= 0; i--) {
      octets[i] += n
      if (octets[i] > 255) {
        n = octets[i] >> 8
        octets[i] &= 255
      } else {
        break
      }
    }
    if (octets[0] > 255) {
      throw new Error('overflow incrementing ip')
    }
    return IpAddress.fromOctets(octets)
  }
  /**
   * Returns a new IpAddress decremented by `n`.
   * @param n - The integer amount to subtract (fractional part is truncated)
   * @returns A new IpAddress with the result
   * @throws Error on underflow
   */
  sub(n: number): IpAddress {
    let octets = [...this.octets]
    n = Math.floor(n)
    for (let i = octets.length - 1; i >= 0; i--) {
      octets[i] -= n
      if (octets[i] < 0) {
        n = Math.ceil(Math.abs(octets[i]) / 256)
        octets[i] = ((octets[i] % 256) + 256) % 256
      } else {
        break
      }
    }
    if (octets[0] < 0) {
      throw new Error('underflow decrementing ip')
    }
    return IpAddress.fromOctets(octets)
  }
  /**
   * Compares this address to another, returning -1, 0, or 1.
   * @param other - An IpAddress instance or string to compare against
   * @returns -1 if this < other, 0 if equal, 1 if this > other
   */
  cmp(other: string | IpAddress): -1 | 0 | 1 {
    if (typeof other === 'string') other = IpAddress.parse(other)
    const len = Math.max(this.octets.length, other.octets.length)
    for (let i = 0; i < len; i++) {
      const left = this.octets[i] || 0
      const right = other.octets[i] || 0
      if (left > right) {
        return 1
      } else if (left < right) {
        return -1
      }
    }
    return 0
  }
  /** The string representation of this IP address (e.g. `"192.168.1.1"` or `"::1"`). Cached and recomputed only when octets change. */
  get address(): string {
    if (
      this.renderedOctets.length === this.octets.length &&
      this.renderedOctets.every((o, idx) => o === this.octets[idx])
    ) {
      // already rendered
    } else if (this.octets.length === 4) {
      this.renderedAddress = this.octets.join('.')
      this.renderedOctets = [...this.octets]
    } else if (this.octets.length === 16) {
      this.renderedAddress = renderIpv6(this.octets)
      this.renderedOctets = [...this.octets]
    } else {
      console.warn('invalid octet length for IpAddress', this.octets)
    }
    return this.renderedAddress
  }
}

/**
 * Represents an IP network (CIDR notation) combining an IP address with a prefix length.
 * Extends IpAddress with network-specific operations like containment checks and broadcast calculation.
 *
 * @example
 * ```ts
 * const net = IpNet.parse("192.168.1.0/24")
 * net.contains("192.168.1.100") // true
 * net.broadcast() // 192.168.1.255
 * ```
 */
export class IpNet extends IpAddress {
  private constructor(
    octets: number[],
    public prefix: number,
    address: string,
  ) {
    super(octets, address)
  }
  /**
   * Creates an IpNet from an IpAddress and prefix length.
   * @param ip - The base IP address
   * @param prefix - The CIDR prefix length (0-32 for IPv4, 0-128 for IPv6)
   * @returns A new IpNet instance
   * @throws Error if prefix exceeds the address bit length
   */
  static fromIpPrefix(ip: IpAddress, prefix: number): IpNet {
    if (prefix > ip.octets.length * 8) {
      throw new Error('invalid prefix')
    }
    return new IpNet(ip.octets, prefix, ip.address)
  }
  /**
   * Parses a CIDR notation string (e.g. `"192.168.1.0/24"`) into an IpNet.
   * @param ipnet - The CIDR string to parse
   * @returns A new IpNet instance
   */
  static parse(ipnet: string): IpNet {
    const [address, prefixStr] = ipnet.split('/', 2)
    const ip = IpAddress.parse(address)
    const prefix = Number(prefixStr)
    return IpNet.fromIpPrefix(ip, prefix)
  }
  /**
   * Checks whether this network contains the given address or subnet.
   * @param address - An IP address or subnet (string, IpAddress, or IpNet)
   * @returns True if the address falls within this network's range
   */
  contains(address: string | IpAddress | IpNet): boolean {
    if (typeof address === 'string') address = IpAddress.parse(address)
    if (address instanceof IpNet && address.prefix < this.prefix) return false
    if (this.octets.length !== address.octets.length) return false
    let prefix = this.prefix
    let idx = 0
    while (idx < this.octets.length && prefix >= 8) {
      if (this.octets[idx] !== address.octets[idx]) {
        return false
      }
      idx += 1
      prefix -= 8
    }
    if (prefix === 0 || idx >= this.octets.length) return true
    const mask = 255 ^ (255 >> prefix)
    return (this.octets[idx] & mask) === (address.octets[idx] & mask)
  }
  /** Returns the network address (all host bits zeroed) for this subnet. */
  zero(): IpAddress {
    let octets: number[] = []
    let prefix = this.prefix
    for (let idx = 0; idx < this.octets.length; idx++) {
      if (prefix >= 8) {
        octets[idx] = this.octets[idx]
        prefix -= 8
      } else {
        const mask = 255 ^ (255 >> prefix)
        octets[idx] = this.octets[idx] & mask
        prefix = 0
      }
    }

    return IpAddress.fromOctets(octets)
  }
  /** Returns the broadcast address (all host bits set to 1) for this subnet. */
  broadcast(): IpAddress {
    let octets: number[] = []
    let prefix = this.prefix
    for (let idx = 0; idx < this.octets.length; idx++) {
      if (prefix >= 8) {
        octets[idx] = this.octets[idx]
        prefix -= 8
      } else {
        const mask = 255 >> prefix
        octets[idx] = this.octets[idx] | mask
        prefix = 0
      }
    }

    return IpAddress.fromOctets(octets)
  }
  /** The CIDR notation string for this network (e.g. `"192.168.1.0/24"`). */
  get ipnet() {
    return `${this.address}/${this.prefix}`
  }
}

/** All private IPv4 ranges: loopback (127.0.0.0/8), Class A (10.0.0.0/8), Class B (172.16.0.0/12), Class C (192.168.0.0/16). */
export const PRIVATE_IPV4_RANGES = [
  IpNet.parse('127.0.0.0/8'),
  IpNet.parse('10.0.0.0/8'),
  IpNet.parse('172.16.0.0/12'),
  IpNet.parse('192.168.0.0/16'),
]

/** IPv4 loopback network (127.0.0.0/8). */
export const IPV4_LOOPBACK = IpNet.parse('127.0.0.0/8')
/** IPv6 loopback address (::1/128). */
export const IPV6_LOOPBACK = IpNet.parse('::1/128')
/** IPv6 link-local network (fe80::/10). */
export const IPV6_LINK_LOCAL = IpNet.parse('fe80::/10')
/** IPv6 unique-local network (fc00::/7). */
export const IPV6_ULA = IpNet.parse('fc00::/7')

/** Carrier-Grade NAT (CGNAT) address range (100.64.0.0/10), per RFC 6598. */
export const CGNAT = IpNet.parse('100.64.0.0/10')

/**
 * Render 16 IPv6 octets to a canonical string: eight 16-bit groups in lowercase
 * hex, with the longest run of two or more all-zero groups collapsed to `::`.
 */
function renderIpv6(octets: number[]): string {
  const groups: number[] = []
  for (let i = 0; i < 16; i += 2) {
    groups.push((octets[i] << 8) | octets[i + 1])
  }
  let bestStart = -1
  let bestLen = 0
  let curStart = -1
  let curLen = 0
  for (let i = 0; i <= groups.length; i++) {
    if (i < groups.length && groups[i] === 0) {
      if (curStart < 0) curStart = i
      curLen++
    } else {
      if (curLen > bestLen) {
        bestLen = curLen
        bestStart = curStart
      }
      curStart = -1
      curLen = 0
    }
  }
  const hex = groups.map(g => g.toString(16))
  return bestLen >= 2
    ? `${hex.slice(0, bestStart).join(':')}::${hex.slice(bestStart + bestLen).join(':')}`
    : hex.join(':')
}
