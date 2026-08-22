import { inject } from '@angular/core'
import { AbstractControl, ValidationErrors } from '@angular/forms'
import { i18nPipe } from '../i18n/i18n.pipe'

// The root CA's Common Name is `<hostname> Local Root CA`, and X.509 caps a
// Common Name at 64 characters.
const MAX_LENGTH = 50
const CHARACTERS = /^[a-z0-9-]+$/

/**
 * Applies the rules the server applies in `ServerHostname::new_from_input`,
 * ignoring surrounding whitespace, and reports an empty value as `required`.
 * Submit the trimmed value.
 */
export function hostnameValidator(
  control: AbstractControl,
): ValidationErrors | null {
  const hostname: string = (control.value || '').trim()
  if (!hostname) return { required: true }

  if (!CHARACTERS.test(hostname)) return { hostnameCharacters: true }
  if (hostname.length > MAX_LENGTH) return { hostnameMaxLength: true }
  if (hostname.startsWith('-') || hostname.endsWith('-')) {
    return { hostnameHyphenEdge: true }
  }

  return null
}

/**
 * Maps `hostnameValidator`'s errors to their messages. Call it inside an
 * injection context — pass `tuiValidationErrorsProvider` a factory, not an object.
 */
export function hostnameValidationErrors(): Record<string, string> {
  const i18n = inject(i18nPipe)

  return {
    required: i18n.transform('Required'),
    hostnameCharacters: i18n.transform(
      'Lowercase letters, numbers, and hyphens only',
    ),
    hostnameMaxLength: i18n.transform('Must be 50 characters or less'),
    hostnameHyphenEdge: i18n.transform('Cannot start or end with a hyphen'),
  }
}
