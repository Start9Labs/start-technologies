/**
 * A wrapper around RegExp that supports composition into larger patterns.
 * Provides helpers to produce anchored (full-match), grouped (sub-expression),
 * and unanchored (contains) regex source strings.
 *
 * @example
 * ```ts
 * const digit = new ComposableRegex(/\d+/)
 * digit.matches()  // "^\\d+$"
 * digit.contains() // "\\d+"
 * digit.asExpr()   // "(\\d+)"
 * ```
 */
export class ComposableRegex {
  readonly regex: RegExp
  constructor(regex: RegExp | string) {
    if (regex instanceof RegExp) {
      this.regex = regex
    } else {
      this.regex = new RegExp(regex)
    }
  }
  /** Returns the regex source wrapped in a capturing group, suitable for embedding in a larger expression. */
  asExpr(): string {
    return `(${this.regex.source})`
  }
  /** Returns the regex source anchored with `^...$` for full-string matching. */
  matches(): string {
    return `^${this.regex.source}$`
  }
  /** Returns the raw regex source string for substring/containment matching. */
  contains(): string {
    return this.regex.source
  }
}

/**
 * Escapes all regex special characters in a string so it can be used as a literal in a RegExp.
 * @param str - The string to escape
 * @returns The escaped string safe for regex interpolation
 */
export const escapeLiteral = (str: string) =>
  str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

/** Composable regex for matching IPv6 addresses (all standard forms including `::` shorthand). */
// https://ihateregex.io/expr/ipv6/
export const ipv6 = new ComposableRegex(
  /(([0-9a-fA-F]{1,4}:){7,7}[0-9a-fA-F]{1,4}|([0-9a-fA-F]{1,4}:){1,7}:|([0-9a-fA-F]{1,4}:){1,6}:[0-9a-fA-F]{1,4}|([0-9a-fA-F]{1,4}:){1,5}(:[0-9a-fA-F]{1,4}){1,2}|([0-9a-fA-F]{1,4}:){1,4}(:[0-9a-fA-F]{1,4}){1,3}|([0-9a-fA-F]{1,4}:){1,3}(:[0-9a-fA-F]{1,4}){1,4}|([0-9a-fA-F]{1,4}:){1,2}(:[0-9a-fA-F]{1,4}){1,5}|[0-9a-fA-F]{1,4}:((:[0-9a-fA-F]{1,4}){1,6})|:((:[0-9a-fA-F]{1,4}){1,7}|:)|fe80:(:[0-9a-fA-F]{0,4}){0,4}%[0-9a-zA-Z]{1,}|::(ffff(:0{1,4}){0,1}:){0,1}((25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])\.){3,3}(25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])|([0-9a-fA-F]{1,4}:){1,4}:((25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])\.){3,3}(25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9]))/,
)

/** Composable regex for matching IPv4 addresses in dotted-decimal notation. */
// https://ihateregex.io/expr/ipv4/
export const ipv4 = new ComposableRegex(
  /(\b25[0-5]|\b2[0-4][0-9]|\b[01]?[0-9][0-9]?)(\.(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)){3}/,
)

/** Composable regex for matching RFC-compliant hostnames. */
export const hostname = new ComposableRegex(
  /(([a-zA-Z0-9]|[a-zA-Z0-9][a-zA-Z0-9\-]*[a-zA-Z0-9])\.)*([A-Za-z0-9]|[A-Za-z0-9][A-Za-z0-9\-]*[A-Za-z0-9])/,
)

/** Composable regex for matching `.local` mDNS hostnames. */
export const localHostname = new ComposableRegex(
  /[-a-zA-Z0-9@:%._\+~#=]{1,256}\.local/,
)

/** Composable regex for matching HTTP/HTTPS URLs. */
// https://ihateregex.io/expr/url/
export const url = new ComposableRegex(
  /https?:\/\/(www\.)?[-a-zA-Z0-9@:%._\+~#=]{1,256}\.[a-zA-Z0-9()]{1,6}\b([-a-zA-Z0-9()!@:%_\+.~#?&\/\/=]*)/,
)

/** Composable regex for matching `.local` URLs (mDNS/LAN). */
export const localUrl = new ComposableRegex(
  /https?:\/\/(www\.)?[-a-zA-Z0-9@:%._\+~#=]{1,256}\.local\b([-a-zA-Z0-9()!@:%_\+.~#?&\/\/=]*)/,
)

/** Composable regex for matching printable ASCII characters (space through tilde). */
// https://ihateregex.io/expr/ascii/
export const ascii = new ComposableRegex(/[ -~]*/)

/**
 * Composable regex for matching fully qualified domain names.
 *
 * Each label is 1-63 characters, starts and ends alphanumeric, and may contain
 * hyphens internally. The final label must start with a letter, which keeps a
 * dotted-decimal IPv4 literal from parsing as a domain (RFC 1123 § 2.1) while
 * still admitting digits and hyphens in the TLD, as IDN A-labels (`xn--p1ai`)
 * and non-delegated private TLDs both require.
 */
export const domain = new ComposableRegex(
  /([A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?\.)+[A-Za-z](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?/,
)

/** Composable regex for matching email addresses. */
// https://www.regular-expressions.info/email.html
export const email = new ComposableRegex(`[A-Za-z0-9._%+-]+@${domain.asExpr()}`)

/** Composable regex for matching email addresses optionally preceded by a display name (e.g. `"Name <email>"`). */
export const emailWithName = new ComposableRegex(
  `${email.asExpr()}|([^<]*<${email.asExpr()}>)`,
)

/** Composable regex for matching base64-encoded strings (no whitespace). */
//https://rgxdb.com/r/1NUN74O6
export const base64 = new ComposableRegex(
  /(?:[a-zA-Z0-9+\/]{4})*(?:|(?:[a-zA-Z0-9+\/]{3}=)|(?:[a-zA-Z0-9+\/]{2}==)|(?:[a-zA-Z0-9+\/]{1}===))/,
)

/** Composable regex for matching base64-encoded strings that may contain interspersed whitespace. */
//https://rgxdb.com/r/1NUN74O6
export const base64Whitespace = new ComposableRegex(
  /(?:([a-zA-Z0-9+\/]\s*){4})*(?:|(?:([a-zA-Z0-9+\/]\s*){3}=)|(?:([a-zA-Z0-9+\/]\s*){2}==)|(?:([a-zA-Z0-9+\/]\s*){1}===))/,
)

/**
 * Creates a composable regex for matching PEM-encoded blocks with the given label.
 * @param label - The PEM label (e.g. `"CERTIFICATE"`, `"RSA PRIVATE KEY"`)
 * @returns A ComposableRegex matching `-----BEGIN <label>-----...-----END <label>-----`
 */
export const pem = (label: string) =>
  new ComposableRegex(
    `-----BEGIN ${escapeLiteral(label)}-----\r?\n[a-zA-Z0-9+/\n\r=]*?\r?\n-----END ${escapeLiteral(label)}-----`,
  )
