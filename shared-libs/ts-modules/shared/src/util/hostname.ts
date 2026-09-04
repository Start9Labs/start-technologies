import { inject } from '@angular/core'
import { AbstractControl, ValidationErrors } from '@angular/forms'
import { i18nPipe } from '../i18n/i18n.pipe'

const MAX_LENGTH = 32
const CHARACTERS = /^[a-z0-9-]+$/

/** Validates after trimming; callers must submit the trimmed value. */
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

/** Returns localized hostname errors. Must run within an injection context. */
export function hostnameValidationErrors(): Record<string, string> {
  const i18n = inject(i18nPipe)

  return {
    required: i18n.transform('Required'),
    hostnameCharacters: i18n.transform(
      'Lowercase letters, numbers, and hyphens only',
    ),
    hostnameMaxLength: i18n.transform('Must be 32 characters or less'),
    hostnameHyphenEdge: i18n.transform('Cannot start or end with a hyphen'),
  }
}
