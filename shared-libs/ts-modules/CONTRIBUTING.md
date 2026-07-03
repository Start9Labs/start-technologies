# Contributing to StartOS Web

For general environment setup (Node.js, cloning, etc.), see the root [CONTRIBUTING.md](../CONTRIBUTING.md).

## Documentation

This directory's docs split across four files:

- `README.md` — what this is
- `ARCHITECTURE.md` — how it's built
- `CONTRIBUTING.md` — this file; how to contribute
- `AGENTS.md` — AI-developer / day-to-day operating rules (`CLAUDE.md` is a one-line `@AGENTS.md` import)

**These docs must be kept up to date.** When you change project structure, conventions, build process, or product context, update the relevant file(s) in the same change — do not defer.

## Prerequisites

From the repo root (the Angular workspace is rooted there):

```sh
npm ci
npm run build:deps   # builds @start9labs/start-core (make dist) + patch-db client; both are file: deps
```

#### Configure `config.json`

```sh
cp shared-libs/ts-modules/config-sample.json config.json
```

- By default, "useMocks" is set to `true`.
- Use "maskAs" to mock the host from which the web UI is served. Valid values are `tor`, `local`, `localhost`, `ipv4`, `ipv6`, and `clearnet`.
- Use "maskAsHttps" to mock the protocol over which the web UI is served. `true` means https; `false` means http.

## Development Server

You can develop using mocks (recommended to start) or against a live server. Code changes will live reload the browser.

### Using mocks

```sh
npm run start:setup
npm run start:ui
```

### Proxying to a live server

1. In `config.json`, set "useMocks" to `false`

2. Copy and configure the proxy config:

```sh
cp proxy.conf-sample.json proxy.conf.json
```

3. Replace every instance of `<CHANGEME>` with the hostname of your remote server

4. Start the proxy dev server:

```sh
npm run start:ui:proxy
```

## Building

Run from the repo root:

```sh
npm run build:ui       # production build of a single app (build:setup / build:tunnel / build:brochure)
```

## Testing

There is no unit-test runner wired up — type-checking (tsc, strict + strictTemplates) plus a successful build is the verification bar. Run from the repo root:

```sh
npm run check          # type-check every project (i18n, shared, marketplace, ui, setup, brochure)
npm run check:ui       # or a single project (check:shared / check:marketplace / check:setup / check:tunnel / check:brochure)
```

`npm run check:i18n` validates that every language dictionary stays in sync with `en.ts`.

## Formatting

Run from the repo root:

```sh
make web-format         # prettier --write across the libs and all app dirs
make web-format-check   # prettier --check (CI)
```

A pre-commit hook runs `lint-staged` (prettier on staged files).

## Translations

### Currently supported languages

- English
- Spanish
- Polish
- German
- French
<!-- - Korean
- Russian
- Japanese
- Hebrew
- Arabic
- Mandarin
- Hindi
- Portuguese
- Italian
- Thai -->

### Adding a new translation

When prompting AI to translate the English dictionary, it is recommended to only give it 50-100 entries at a time. Beyond that it struggles. Remember to sanity check the results and ensure keys/values align in the resulting dictionary.

#### Sample AI prompt

Translate the English dictionary below into `<language>`. Format the result as a javascript object with the numeric values of the English dictionary as keys in the translated dictionary. These translations are for the web UI of StartOS, a graphical server operating system optimized for self-hosting. Comments may be included in the English dictionary to provide additional context.

#### Adding to StartOS

- In the `shared` project:
  1. Create a new file (`language.ts`) in `src/i18n/dictionaries`
  2. Update the `I18N_PROVIDERS` array in `src/i18n/i18n.providers.ts` (2 places)
  3. Update the `languages` array in `src/i18n/i18n.service.ts`
  4. Add the name of the new language (lowercase) to the English dictionary in `src/i18n/dictionaries/en.ts`. Add the translations of the new language's name (lowercase) to ALL non-English dictionaries in `src/i18n/dictionaries/` (e.g., `es.ts`, `pl.ts`, etc.).

  If you have any doubt about the above steps, check the [French example PR](https://github.com/Start9Labs/start-technologies/pull/2945/files) for reference.

- Here in this CONTRIBUTING.md:
  1. Add the language to the list of supported languages above

### Updating the English dictionary

#### Sample AI prompt

Translate the English dictionary below into the languages beneath the dictionary. Format the result as a javascript object with translated language as keys, mapping to a javascript object with the numeric values of the English dictionary as keys and the translations as values. These translations are for the web UI of StartOS, a graphical server operating system optimized for self-hosting. Comments may be included in the English dictionary to provide additional context.

English dictionary:

```
'Hello': 420,
'Goodby': 421
```

Languages:

- Spanish
- Polish
- German
- French

#### Adding to StartOS

In the `shared` project, copy/paste the translations into their corresponding dictionaries in `src/i18n/dictionaries`.
