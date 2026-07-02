import { computed, inject, Provider, Signal } from '@angular/core'
import { tuiValidationErrorsProvider } from '@taiga-ui/core'
import { PolymorpheusContent } from '@taiga-ui/polymorpheus'
import { i18nPipe } from './i18n.pipe'
import { i18nKey } from './i18n.providers'

/**
 * An interpolated validation message. `template` is an English string registered
 * in `en.ts` containing `{placeholder}` tokens; `resolve` maps the validator's
 * error context to the substitution values.
 */
export interface TranslatedTemplate<Ctx = any> {
  readonly template: i18nKey
  readonly resolve: (ctx: Ctx) => Record<string, string | number>
}

/**
 * A single validation message: a plain key, a context function returning a key
 * (for multi-message switches), or an interpolated `tpl(...)` template.
 */
export type TranslatedError =
  | i18nKey
  | ((ctx: any) => i18nKey)
  | TranslatedTemplate

export type TranslatedErrors = Record<string, TranslatedError>

/** Declare an interpolated validation message. */
export function tpl<Ctx>(
  template: i18nKey,
  resolve: (ctx: Ctx) => Record<string, string | number>,
): TranslatedTemplate<Ctx> {
  return { template, resolve }
}

function isTemplate(value: TranslatedError): value is TranslatedTemplate {
  return typeof value === 'object' && value !== null && 'template' in value
}

export function fill(
  text: string,
  vars: Record<string, string | number>,
): string {
  return text.replace(/\{(\w+)\}/g, (_, key) =>
    key in vars ? String(vars[key]) : `{${key}}`,
  )
}

/**
 * Drop-in replacement for `tuiValidationErrorsProvider` that routes every
 * message through the i18n pipe, so `<tui-error>` text is translated and
 * re-translates live on language change.
 *
 * Mirrors start-os's `TUI_VALIDATION_ERRORS` useFactory + injected `i18nPipe`
 * idiom; plain-string entries become reactive `computed` signals (Taiga accepts
 * `Signal<PolymorpheusContent>`), and function/template entries translate inside
 * Taiga's own error computed so they react to the language signal too.
 */
export function provideTranslatedValidationErrors(
  errors: TranslatedErrors,
): Provider {
  return tuiValidationErrorsProvider(() => {
    const i18n = inject(i18nPipe)
    const out: Record<
      string,
      PolymorpheusContent | Signal<PolymorpheusContent>
    > = {}

    for (const [key, value] of Object.entries(errors)) {
      if (isTemplate(value)) {
        out[key] = (ctx: any) =>
          fill(i18n.transform(value.template), value.resolve(ctx))
      } else if (typeof value === 'function') {
        out[key] = (ctx: any) => i18n.transform(value(ctx))
      } else {
        out[key] = computed(() => i18n.transform(value))
      }
    }

    return out
  })
}
