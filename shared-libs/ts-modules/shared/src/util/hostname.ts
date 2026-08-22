import { inject } from '@angular/core'
import { AbstractControl, ValidationErrors } from '@angular/forms'
import { i18nPipe } from '../i18n/i18n.pipe'

// The longest label DNS allows, and so the longest a `.local` name can be.
const MAX_LENGTH = 63
const CHARACTERS = /^[a-z0-9-]+$/

/**
 * Rejects a hostname `ServerHostname::new_from_input` would reject on the server,
 * and reports an empty one as `required`. It ignores surrounding whitespace, so
 * submit the trimmed value.
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
 * The messages `hostnameValidator`'s errors render as. Call it inside an
 * injection context — pass `tuiValidationErrorsProvider` a factory, not an object.
 */
export function hostnameValidationErrors(): Record<string, string> {
  const i18n = inject(i18nPipe)

  return {
    hostnameCharacters: i18n.transform(
      'Lowercase letters, numbers, and hyphens only',
    ),
    hostnameMaxLength: i18n.transform('Must be 63 characters or less'),
    hostnameHyphenEdge: i18n.transform('Cannot start or end with a hyphen'),
  }
}
