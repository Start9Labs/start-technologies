import { domain } from '../util/regexes'

const matches = new RegExp(domain.matches())

describe('the domain regex', () => {
  test.each([
    'db.com',
    'sub.domain.example.com',
    'nextcloud.private',
    'private.domain.internal',
    'nextcloud.fake-tld', // the example in private-domains.md
    'nextcloud.lan2', // a non-delegated TLD containing digits
    'example.xn--p1ai', // IDN A-label TLD (.рф)
    'a.co',
  ])('accepts %s', fqdn => {
    expect(matches.test(fqdn)).toBe(true)
  })

  test.each([
    ['1.2.3.4', 'a dotted-decimal IPv4 literal, RFC 1123 § 2.1'],
    ['nodot', 'a single label is not fully qualified'],
    ['foo..com', 'an empty label'],
    ['..foo.com', 'a leading dot'],
    ['-foo.com', 'a label starting with a hyphen'],
    ['foo-.com', 'a label ending with a hyphen'],
    [`${'a'.repeat(64)}.com`, 'a label over 63 characters'],
  ])('rejects %s (%s)', fqdn => {
    expect(matches.test(fqdn)).toBe(false)
  })
})
