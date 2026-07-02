import { AbstractControl, ValidationErrors, ValidatorFn } from '@angular/forms'

export class CustomValidators {
  static ipv4(): ValidatorFn {
    return (control: AbstractControl): ValidationErrors | null => {
      if (!control.value) return null

      const ipv4Regex = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/
      const match = control.value.match(ipv4Regex)

      if (!match) return { ipv4: true }

      // Validate each octet is 0-255
      const octets = [match[1], match[2], match[3], match[4]]
      const valid = octets.every(octet => {
        const num = parseInt(octet, 10)
        return num >= 0 && num <= 255
      })

      return valid ? null : { ipv4: true }
    }
  }

  static ipv6(): ValidatorFn {
    return (control: AbstractControl): ValidationErrors | null => {
      if (!control.value) return null

      // Simplified IPv6 validation (full validation is complex)
      const ipv6Regex =
        /^(([0-9a-fA-F]{1,4}:){7}[0-9a-fA-F]{1,4}|([0-9a-fA-F]{1,4}:){1,7}:|([0-9a-fA-F]{1,4}:){1,6}:[0-9a-fA-F]{1,4}|([0-9a-fA-F]{1,4}:){1,5}(:[0-9a-fA-F]{1,4}){1,2}|([0-9a-fA-F]{1,4}:){1,4}(:[0-9a-fA-F]{1,4}){1,3}|([0-9a-fA-F]{1,4}:){1,3}(:[0-9a-fA-F]{1,4}){1,4}|([0-9a-fA-F]{1,4}:){1,2}(:[0-9a-fA-F]{1,4}){1,5}|[0-9a-fA-F]{1,4}:((:[0-9a-fA-F]{1,4}){1,6})|:((:[0-9a-fA-F]{1,4}){1,7}|:))$/

      return ipv6Regex.test(control.value) ? null : { ipv6: true }
    }
  }

  static ipv4OrIpv6(): ValidatorFn {
    return (control: AbstractControl): ValidationErrors | null => {
      if (!control.value) return null

      const ipv4Result = CustomValidators.ipv4()(control)
      const ipv6Result = CustomValidators.ipv6()(control)

      // Valid if either IPv4 or IPv6 is valid
      return ipv4Result === null || ipv6Result === null ? null : { ip: true }
    }
  }

  static prefix(min: number, max: number): ValidatorFn {
    return (control: AbstractControl): ValidationErrors | null => {
      if (!control.value) return null

      const value = control.value.replace(/^\//, '')
      const num = parseInt(value, 10)

      if (isNaN(num) || num < min || num > max) {
        return { prefix: { min, max } }
      }

      return null
    }
  }

  static mac(): ValidatorFn {
    return (control: AbstractControl): ValidationErrors | null => {
      if (!control.value) return null

      // MAC address format: XX:XX:XX:XX:XX:XX or XX-XX-XX-XX-XX-XX
      const macRegex = /^([0-9A-Fa-f]{2}[:-]){5}([0-9A-Fa-f]{2})$/

      return macRegex.test(control.value) ? null : { mac: true }
    }
  }

  static hostname(): ValidatorFn {
    return (control: AbstractControl): ValidationErrors | null => {
      if (!control.value) return null

      // RFC 1123: letters, digits, hyphens; no leading/trailing hyphen; max 63 chars
      const hostnameRegex = /^[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?$/

      return hostnameRegex.test(control.value) ? null : { hostname: true }
    }
  }

  static duplicateSubnet(used: number[]): ValidatorFn {
    return (control: AbstractControl): ValidationErrors | null => {
      if (control.value == null) return null
      return used.includes(control.value) ? { duplicateSubnet: true } : null
    }
  }

  static duplicateName(existing: string[]): ValidatorFn {
    const names = new Set(existing.map(n => n.trim().toLowerCase()))
    return (control: AbstractControl): ValidationErrors | null => {
      if (!control.value) return null
      return names.has(control.value.trim().toLowerCase())
        ? { duplicateName: true }
        : null
    }
  }

  /**
   * Linux interface names are limited to 15 chars (IFNAMSIZ = 16 including NUL).
   * The backend prefixes with "wg_" (3 chars), leaving 12 for the sanitized label.
   * Sanitization: lowercase, non-alphanumeric → underscore.
   */
  static interfaceNameLength(prefix: string, maxTotal: number): ValidatorFn {
    const maxSanitized = maxTotal - prefix.length
    return (control: AbstractControl): ValidationErrors | null => {
      if (!control.value) return null
      const sanitized = control.value.toLowerCase().replace(/[^a-z0-9]/g, '_')
      return sanitized.length > maxSanitized
        ? {
            interfaceNameLength: {
              max: maxTotal,
              prefix,
              current: prefix.length + sanitized.length,
            },
          }
        : null
    }
  }

  static ipv4List(): ValidatorFn {
    return (control: AbstractControl): ValidationErrors | null => {
      if (!control.value) return null

      const items = control.value
        .split(',')
        .map((s: string) => s.trim())
        .filter(Boolean)

      for (const item of items) {
        // Check if it's a CIDR (IP/prefix)
        if (item.includes('/')) {
          const [ip, prefix] = item.split('/')
          if (!isValidIpv4(ip)) return { ipv4List: true }
          const prefixNum = parseInt(prefix, 10)
          if (isNaN(prefixNum) || prefixNum < 0 || prefixNum > 32)
            return { ipv4List: true }
        } else {
          // Plain IP
          if (!isValidIpv4(item)) return { ipv4List: true }
        }
      }
      return null
    }
  }
}

function isValidIpv4(ip: string): boolean {
  const ipv4Regex = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/
  const match = ip.match(ipv4Regex)
  if (!match) return false
  return [match[1], match[2], match[3], match[4]].every(octet => {
    const num = parseInt(octet, 10)
    return num >= 0 && num <= 255
  })
}
