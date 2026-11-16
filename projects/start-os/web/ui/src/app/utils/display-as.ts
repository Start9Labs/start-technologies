import Mustache from 'mustache'

export function renderDisplayAs(template: string, value: unknown): string {
  return Mustache.render(template, value, undefined, { escape: String })
}
