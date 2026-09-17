# Forms

- **`inject(NonNullableFormBuilder).group({...})` with array shorthand** as a field initializer;
  types are inferred, never annotated. Inline literal narrowing where needed
  (`protocol: ['tcp' as Protocol]`). `FormBuilder`/untyped `FormControl`: banned (the one
  exception: StartOS's spec-driven dynamic form engine legitimately uses `UntypedFormBuilder`).

```ts
protected readonly form = inject(NonNullableFormBuilder).group({
  code: [inject(ActivatedRoute).snapshot.queryParamMap.get('code') || '', Validators.required],
  email: ['', [Validators.required, Validators.pattern(email)]],
})
```

- **Field anatomy** (the v5 textfield composition API — legacy `tui-input-*` components are
  extinct):

```html
<form tuiForm="m" [formGroup]="form" (submit.prevent)="save()">
  <tui-textfield>
    <label tuiLabel>Email</label>
    <input tuiInput formControlName="email" />
  </tui-textfield>
  <tui-error formControlName="email" />
  <footer>
    <button tuiButton appearance="flat" type="button" (click)="context.$implicit.complete()">Cancel</button>
    <button tuiButton [disabled]="form.invalid" [loading]="saving()">Save</button>
  </footer>
</form>
```

Password reveal: `<tui-icon tuiPassword />` inside the textfield — **import `TuiIcon`
alongside `TuiPassword`**, or `TuiPassword` matches the bare element name, `TuiIcons`
never applies, and the page dies at runtime with `NG0201: No provider for TuiIcons`
that `strictTemplates` cannot see. Selects:
`<tui-textfield tuiChevron [stringify]="fn"><input tuiSelect /><tui-data-list *tuiDropdown>…`
(or `<tui-data-list-wrapper *tuiDropdown [items]="…" />`).

- **Error messages are declarative and central**: bare `<tui-error formControlName="x" />` +
  `tuiValidationErrorsProvider({...})` — at root for a monolingual app, in **component
  `providers`** per dialog/page, or via a wrapper that routes messages through i18n
  (`provideTranslatedValidationErrors` in start-wrt, messages as reactive `computed` signals).
  Never per-field message markup; never the legacy `[error]="[] | tuiFieldError | async"` chain.
- **Cross-field validation is declarative**: `[tuiValidator]="form.value.password || '' |
tuiMapper: match"` with a `match(pw): ValidatorFn` factory — or plain validator functions on
  the group. Imperative `setValidators` juggling in lifecycle hooks is tracked debt.
- **Submit**: prefer an **enabled** submit that runs
  `tuiMarkControlAsTouchedAndValidate(this.form)` and bails if invalid — from review: _"a good
  UX pattern is to not disable it… so that I can see all the fields I forgot to type in."_
  `[disabled]="form.invalid"` is acceptable on tiny one/two-field forms. Async flow is the
  signal + try/catch/finally with a toast on failure, or `TaskService.run`. Zoneless-safe
  validity: `formStatus = toSignal(this.form.statusChanges, { initialValue: this.form.status })`;
  a full-page form's Cancel is `type="reset"` disabled on `form.pristine`.
- **Single ad-hoc fields skip reactive forms**: `[(ngModel)]="signal"`
  (+ `[ngModelOptions]="{standalone: true}"` inside a formGroup context), `linkedSignal`
  clearing the error on edit.
- **Masking**: Maskito (`@maskito/*`) where real masking is needed (start-wrt IP masks).
