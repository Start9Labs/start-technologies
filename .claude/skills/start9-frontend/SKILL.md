---
name: start9-frontend
description: Start9 house style for every Angular + Taiga UI web frontend (StartOS ui/setup-wizard, StartWRT, StartTunnel, brochure-marketplace, start9-store, ops-server, support-server). Use BEFORE writing or reviewing any frontend code in these apps — components, templates, styles/CSS, forms, dialogs/toasts, routing, state, DI, i18n — and when bootstrapping a new Start9 web UI or upgrading Angular/Taiga. Includes doctrine, recipes, an antipattern catalog distilled from the fleet's refactor history, and a verified Taiga 5 API reference.
---

# Start9 Frontend — Angular + Taiga UI house style

Canonical for **every Start9 web UI**, distilled from the fleet's codebases and from the
refactor and review history that shaped them. Follow it and new code matches the fleet;
deviate and expect it to be rewritten in review.

**Authority order: Taiga UI docs > this skill > neighboring code.** The codebases are
mid-migration — _"matches existing repo patterns" is not a quality bar_; old code is often
exactly what this skill exists to eliminate. Never guess a Taiga API: verify against the
`taiga-ui-mcp` MCP server or `https://taiga-ui.dev/llms-full.txt`.

## Reference files (read on demand — don't preload)

| File                         | Read when                                                                                                 |
| ---------------------------- | --------------------------------------------------------------------------------------------------------- |
| `references/bootstrap.md`    | creating/touching app.config, main.ts, angular.json, icons, theming, or a new app/repo                    |
| `references/components.md`   | writing any component or template (selectors, hostDirectives, signal APIs, control flow)                  |
| `references/styling.md`      | about to write ANY CSS (the escalation ladder, breakpoints, tokens)                                       |
| `references/di.md`           | providers, tokens, inject patterns                                                                        |
| `references/state.md`        | services, data flow, loading/error UX, API/RPC integration                                                |
| `references/forms.md`        | any form, validation, or submit flow                                                                      |
| `references/overlays.md`     | dialogs, confirms, toasts, dropdowns, hints, drawers                                                      |
| `references/routing.md`      | route files, guards, navigation, URL state                                                                |
| `references/conventions.md`  | i18n strings, file naming, folder structure, import order                                                 |
| `references/shared-libs.md`  | before writing ANY utility/component in monorepo apps — it may exist in @start9labs/shared or marketplace |
| `references/antipatterns.md` | reviewing code, refactoring, or upgrading Angular/Taiga (before→after catalog + verbatim review quotes)   |
| `references/taiga.md`        | unsure of a Taiga 5 API; v4→v5 renames; need→primitive lookup table                                       |
| `references/recipes.md`      | step-by-step: new page, dialog, form, table, control component, endpoint                                  |
| `references/repos.md`        | repo-specific rules and build/verify commands                                                             |

## The fleet

| App                    | Location                                        | Angular | Taiga | Zone                       | Theme                                | i18n               | Backend                                                          |
| ---------------------- | ----------------------------------------------- | ------- | ----- | -------------------------- | ------------------------------------ | ------------------ | ---------------------------------------------------------------- |
| StartOS `ui`           | `start-technologies` `projects/start-os/web/ui` | 22      | 5.11  | zone.js (zoneless pending) | dark, `provideTaiga({mode})`         | yes (shared dicts) | JSON-RPC + PatchDB push                                          |
| `setup-wizard`         | `projects/start-os/web/setup-wizard`            | 22      | 5.11  | zone.js                    | dark                                 | yes (shared)       | JSON-RPC                                                         |
| `start-tunnel`         | `projects/start-tunnel/web`                     | 22      | 5.11  | **zoneless**               | dark                                 | yes (local dicts)  | JSON-RPC + PatchDB                                               |
| `start-wrt`            | `projects/start-wrt/web`                        | 22      | 5.11  | **zoneless**               | dual (`TUI_DARK_MODE`)               | yes (local dicts)  | JSON-RPC, own HTTP stack, 5s polling                             |
| `brochure-marketplace` | `projects/brochure-marketplace`                 | 22      | 5.11  | zone.js (legacy)           | dark                                 | yes (shared)       | registry RPC direct                                              |
| `start9-store`         | `ops/start9-store/web`                          | 22      | 5.22  | **zoneless**               | light                                | no                 | REST + Zod via `/api` BFF, **SSR**                               |
| `ops-server`           | `ops/ops-server/web`                            | 22      | 5.14  | **zoneless**               | dark, `#07a4ff`, Montserrat          | no                 | REST `/_api`, same-origin Express                                |
| `support-server`       | `ops/support-server/web`                        | 22      | 5.22  | **zoneless**               | dual (OS preference), StartOS tokens | yes (local dicts)  | Frappe `/api/method` + socket.io, same-origin; `web/mock` in dev |

TypeScript ~6.0, rxjs ~7.8 everywhere. Taiga is **pinned exact** — bump only with the
maintainer's blessing. Monorepo apps share **one Angular workspace rooted at the repo root**;
ops repos are standalone. Zoneless is the target state — write all new code zoneless-safe.
This table is the **only** place in the fleet where stack versions are written down — update
it with every Angular/Taiga bump (other repos' docs deliberately carry no version specifics).

## Doctrine

1. **Taiga does it all.** Components, layout, forms, dialogs, icons, theming, animation. If
   you're hand-rolling HTML/CSS/JS for something that feels generic, Taiga ships it — look it
   up first. "If you think Taiga can't do something, you're probably wrong."
2. **Never guess a Taiga API.** Taiga 5 is fast-moving and easy to hallucinate. Verify every
   component/directive/token against the docs before use.
3. **Configure the design system, don't fight it.** The escalation ladder for any visual need:
   ① a Taiga primitive/appearance → ② an option provider (root or component `providers`) →
   ③ a `--tui-*` design-token override in the theme sheet → ④ a shared `g-*` utility class →
   ⑤ a few lines of `:host` layout CSS. Reaching for ⑤ first means you missed ①–④.
4. **Delete code.** The canonical refactors are net-negative. The best version has fewer lines,
   fewer files, fewer wrappers, fewer names. Don't restate framework defaults, don't name
   single-use values, don't duplicate markup per breakpoint.
5. **Signals at the component boundary; RxJS composes once, in services.** Components read
   signals; services own streams and convert with `toSignal` at the edge. Data is never
   manually subscribed in a component.
6. **One source of truth per fact.** Nav links live in one object; validation messages in one
   provider map; sizes in option providers; colors in the theme sheet.
7. **Everything host-related goes in the decorator.** `host: {}` for classes/attrs/listeners/
   style bindings, `hostDirectives` for composition. `@HostBinding`/`@HostListener` are dead.
8. **English strings are i18n keys** (monorepo apps): every user-facing string goes through
   `| i18n` and exists in all five dictionaries; `tsc` enforces via the `i18nKey` type.
9. **Verification is `tsc` + Prettier, not tests.** No unit-test runner is wired up anywhere.
   `npm run check` (strict + `strictTemplates`), the i18n check, a prod build, and manual
   verification are the bar. Don't claim "tests pass"; don't add a test framework unasked.
10. **The docs ship with the change — this skill first.** This skill is the fleet-wide
    frontend source of truth: the ops repos reach it through committed symlinks, and stack
    versions live only in its fleet table. When frontend conventions, versions, or idioms
    change, update `SKILL.md` and the affected `references/*.md` in the same change; other
    repos' docs carry only project-specific facts.

## If you learned Angular anywhere else — the surprise index

- **No `NgModule` anywhere** — standalone components only.
- **No `.html`/`.scss` component files.** 100% inline `template:` and `styles:` — one file
  per component.
- **No constructor parameter injection.** `inject()` in field initializers, even chained:
  `protected route = toSignal(inject(Router).events)`.
- **No `ngOnInit`.** Field initializers; a bare `constructor() { this.load() }` kicks off
  fetches; `afterNextRender` for browser-only side effects.
- **No `*ngIf`/`*ngFor`/`ngClass`/`ngStyle`.** `@if`/`@for` (with `track`), `@let`,
  `@switch`; `[class.x]` and unit-typed `[style.prop.unit]` bindings.
- **No `@Input`/`@Output`/`@ViewChild` decorators.** `input()`, `input.required()`,
  `output()`, `model()`, `viewChild()`, `contentChild()`.
- **No `provideAnimations()`.** Taiga 5 animates with CSS (`TuiAnimated`).
- **No explicit event-plugins provider.** `provideTaiga()` bundles it — that's what makes
  `(submit.prevent)`, `(click.self)`, `.stop`, `.capture` modifiers work.
- **No `TuiAlertService`.** Toasts are `TuiNotificationService`; blocking loaders are
  `TuiNotificationMiddleService`.
- **No `FormBuilder`.** `inject(NonNullableFormBuilder).group({...})` with array shorthand;
  `[(ngModel)]="signal"` for single ad-hoc fields.
- **No route-level `providers`,** no resolvers, few guards (inline `canMatch` arrows).
  Providers go on components — lazy-route providers spin up confusing semi-root injectors.
- **No `@media` queries** for the app-standard mobile swap: `tui-root._mobile &` CSS,
  `TUI_BREAKPOINT`/`WA_IS_MOBILE` signals, template `@if` swaps — one DOM, never two.
- **No BEM, no barrels, no `index.ts` re-exports** in apps. Taiga-style `_state` classes and
  shared `g-*` utilities; deep relative or `src/`-absolute imports.
- **No semicolons.** Prettier: `singleQuote`, `semi: false`, `arrowParens: "avoid"`,
  `trailingComma: "all"`, `htmlWhitespaceSensitivity: "ignore"`, `tabWidth: 2`.
- **No ESLint, no unit tests.** Prettier runs via husky/lint-staged — never
  `git commit --no-verify`; fix the formatting.
- **Suffixless files and classes** in new code: `routes/devices/index.ts` exporting
  `export default class Devices`, plus `dialog.ts`, `table.ts`, `service.ts` —
  not `devices-page.component.ts` / `DevicesPageComponent`.
- **Services can _be_ Observables** (`class ConnectionService extends Observable<boolean>`);
  tokens can _be_ signals (`new InjectionToken('…', { factory: () => signal(false) })`).
- **`DOCUMENT` and `@Service()` import from `@angular/core`** now — `@Service()` is the
  emerging norm for new services over `@Injectable({providedIn: 'root'})`.
- **The mock backend is a DI swap**: `useClass: useMocks ? MockApiService : LiveApiService`.

## Review checklist (greppable)

Any hit is a finding unless it matches a documented exception in the reference files:

```
NgModule                    constructor(private           *ngIf / *ngFor
ngClass / ngStyle           @Input( / @Output( / @ViewChild(
@HostBinding / @HostListener
templateUrl / styleUrl
FormBuilder (not NonNullable)   new FormGroup(            : any
provideAnimations           TuiAlertService               tuiFieldError
ngOnInit                    setTimeout                    .subscribe( [outside allowed shapes]
::ng-deep                   !important                    letter-spacing / text-transform
#[0-9a-f]{3,6} [outside the theme sheet]                  @media [outside documented divergences]
window. / document. / localStorage [outside infrastructure]
providers: [ on a route     track $index [on entity lists]
input<T | null>(null)       display: grid on :host of a wrapper around one child
```

Softer review questions: does a `computed` just reshape for the template (→ pipe)? Is a value
named but used once (→ inline)? Is the same appearance/size attribute repeated (→ option
provider)? Is there a second DOM for mobile (→ one DOM + `_mobile` CSS)? Did copy ship in
Title Case (→ sentence case)? Is a Taiga API used that you didn't verify against the docs?
