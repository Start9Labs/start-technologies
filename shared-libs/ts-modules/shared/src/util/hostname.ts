import { inject } from '@angular/core'
import { AbstractControl, ValidationErrors } from '@angular/forms'
import { i18nPipe } from '../i18n/i18n.pipe'

// The longest label DNS allows, and so the longest a `.local` name can be.
const MAX_LENGTH = 63
const CHARACTERS = /^[a-z0-9-]+$/

/**
 * Rejects everything `ServerHostname::validate` rejects in
 * `shared-libs/crates/start-core/src/hostname.rs`, plus the length and
 * hyphen-placement rules a DNS label has to satisfy.
 */
export function hostnameValidator(
  control: AbstractControl,
): ValidationErrors | null {
  const hostname: string = control.value || ''
  if (!hostname) return null

  if (hostname.length > MAX_LENGTH) return { hostnameMaxLength: true }
  if (!CHARACTERS.test(hostname)) return { hostnameCharacters: true }
  if (hostname.startsWith('-') || hostname.endsWith('-')) {
    return { hostnameHyphenEdge: true }
  }

  return null
}

/** The messages `hostnameValidator`'s errors render as, for `TUI_VALIDATION_ERRORS`. */
export function hostnameValidationErrors(): Record<string, string> {
  const i18n = inject(i18nPipe)

  return {
    hostnameMaxLength: i18n.transform('Must be 63 characters or less'),
    hostnameCharacters: i18n.transform(
      'Lowercase letters, numbers, and hyphens only',
    ),
    hostnameHyphenEdge: i18n.transform('Cannot start or end with a hyphen'),
  }
}
