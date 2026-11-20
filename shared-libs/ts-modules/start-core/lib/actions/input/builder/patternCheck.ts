import { Pattern } from '../inputSpecTypes'
import * as z from '../../../z'

/**
 * Anchors a pattern the way Angular's `Validators.pattern` does for a string
 * regex — `^` and `$` are added unless already present. A spec's `regex` is
 * authored against that behavior, so an unanchored test here would accept
 * values the form rejects.
 */
function anchor(regex: string): RegExp {
  return new RegExp(
    `${regex.startsWith('^') ? '' : '^'}${regex}${regex.endsWith('$') ? '' : '$'}`,
  )
}

/**
 * Applies a text field's declared `patterns` to its parser, so the guarantee
 * holds for callers that never render the form — `start-cli`, RPC, anything
 * driving an action directly.
 */
export function withPatterns(
  parser: z.ZodType<string>,
  patterns: Pattern[] | undefined,
): z.ZodType<string> {
  return (patterns ?? []).reduce((acc, { regex, description }) => {
    const anchored = anchor(regex)
    return acc.refine(value => anchored.test(value), {
      message: description,
    })
  }, parser)
}
