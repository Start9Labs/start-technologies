# Contributing to Web

For general setup, see the root [CONTRIBUTING.md](../CONTRIBUTING.md). For architecture details, see [ARCHITECTURE.md](ARCHITECTURE.md).

## Documentation

This sub-tree's docs split across four files:

- `README.md` — what this is
- `ARCHITECTURE.md` — how it's built
- `CONTRIBUTING.md` — this file; how to contribute
- `CLAUDE.md` — AI-developer operating rules (Taiga-first, form patterns)

**These docs must be kept up to date.** When you change web's structure, conventions, build steps, or component patterns, update the relevant file(s) in the same change — do not defer.

## Testing

There is no test framework wired up for this app — the workspace has no `test` runner (Jest/Karma/Vitest) for it. Type-check via `npm run check:wrt`. Don't add a test framework without discussing first.

## Tech Stack

- **Angular 22** with zoneless change detection and standalone components
- **TypeScript 6** in strict mode
- **[Taiga UI v5](https://taiga-ui.dev/next)** component library
- **Signal-based state management** (no external store library)
- **SCSS** for styling, using Taiga CSS variables for theming

## Getting Started

The web is the `start-wrt` project in the root Angular workspace — run everything from the repo root:

```bash
cp projects/start-wrt/web/config-sample.json projects/start-wrt/web/config.json  # One-time: local config
npm ci                                # Install the whole workspace
npm run build:deps                    # Build the file: deps — once after install
npm run start:wrt                     # Dev server with mock API
npm run build:wrt                     # Production build
npm run check:wrt                     # Type-check without emitting
```

### Configuring config.json

`config.json` is **gitignored** — it's generated from `config-sample.json`:

- **Local dev:** `cp config-sample.json config.json`, then edit freely. `npm run start:wrt` runs `build-config.js` first, which stamps the current git hash into `gitHash`.
- **Production / CI:** `make startwrt` triggers `web/update-config.sh`, which flips `useMocks` to `false` and stamps `gitHash` from `build/env/GIT_HASH.txt`.

Schema:

```json
{
  "useMocks": true,
  "api": { "url": "rpc", "version": "v1" },
  "gitHash": ""
}
```

- `useMocks: true` — Uses `MockApiService` (no router needed)
- `useMocks: false` — Uses `LiveApiService` (requires running backend)
- `gitHash` — Stamped at build time; available via `WorkspaceConfig.gitHash` for About / diagnostics UIs.

The API URL resolves to `document.location.origin + /rpc/v1`.

## Adding a New Route

1. **Create the route directory** under `src/app/routes/my-feature/`

2. **Create `index.ts`** — the page component (default export):

```typescript
@Component({
  template: `
    <header tuiHeader="h6"><h2 tuiTitle>My Feature</h2></header>
    <form [formGroup]="form" [formLoading]="!service.data()" ...>
      <!-- fields -->
      @if (service.data()) { <footer appFooter></footer> }
    </form>
  `,
  providers: [provideFormService(MyFeatureService)],
  imports: [/* Taiga + shared components */],
})
export default class MyFeature { ... }
```

3. **Create `service.ts`** — extends `FormService<T>`:

```typescript
@Injectable()
export class MyFeatureService extends FormService<MyData> {
  private readonly api = inject(ApiService)
  async load() {
    return this.api.myFeatureGet()
  }
  async store(data: MyData) {
    await this.api.myFeatureSet(data)
  }
}
```

4. **Register the route** in `app.routes.ts` (or the parent route's children):

```typescript
{ path: 'my-feature', loadComponent: () => import('./routes/my-feature') }
```

5. **Add nav entry** in `components/nav.ts` if it should appear in the sidebar.

## Adding a Dialog

1. **Create `dialog.ts`** in the route folder:

```typescript
@Component({
  template: `
    <form tuiForm="m" [formGroup]="form" (submit.prevent)="save()">
      <!-- fields -->
      <footer>
        <button tuiButton appearance="flat" type="button"
          (click)="context.$implicit.complete()">Cancel</button>
        <button tuiButton>Save</button>
      </footer>
    </form>
  `,
})
export class MyDialog {
  protected readonly context = injectContext<TuiDialogContext<ResultType, InputType>>()
  protected readonly form = inject(NonNullableFormBuilder).group({ ... })

  save() {
    if (this.form.invalid) {
      tuiMarkControlAsTouchedAndValidate(this.form)
      return
    }
    this.context.completeWith(this.form.getRawValue())
  }
}
```

2. **Open it** from the parent component:

```typescript
this.dialogs
  .open<ResultType>(new PolymorpheusComponent(MyDialog), {
    label: 'Edit Thing',
    data: existingItem,
  })
  .subscribe(result => {
    /* handle save */
  })
```

See `routes/published-ports/` for the best complete example of the table + dialog pattern.

## Taiga UI Lookup

When you're unsure how to use a Taiga component:

1. **Check existing code** — search the codebase for usage examples.
2. **Fetch llms-full.txt** — `https://taiga-ui.dev/llms-full.txt` has all components with code examples (~2200 lines). For Claude Code: `WebFetch url=https://taiga-ui.dev/llms-full.txt prompt="How to use TuiTextfield with a select dropdown"`.
3. **MCP server (beta)** — `taiga-family/taiga-ui-mcp` exposes `get_list_components` (fuzzy search) and `get_component_example` (docs + code). Add to your Claude config:

   ```json
   {
     "mcpServers": {
       "taiga-ui": {
         "command": "npx",
         "args": ["-y", "@anthropic-ai/mcp-remote@latest", "https://taiga-ui.dev/next/api/mcp/sse"]
       }
     }
   }
   ```

4. **Taiga docs** — `https://taiga-ui.dev/next`

## Conventions

- All components are standalone (no NgModules)
- Templates and styles are inline in `@Component` (no separate `.html` / `.css` files)
- Always `NonNullableFormBuilder` (never `FormBuilder`)
- Styling uses `g-*` global utility classes and Taiga CSS variables — no Tailwind
