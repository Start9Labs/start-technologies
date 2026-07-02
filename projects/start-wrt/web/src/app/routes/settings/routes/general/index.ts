import {
  Component,
  computed,
  DestroyRef,
  effect,
  inject,
  signal,
} from '@angular/core'
import { NonNullableFormBuilder, ReactiveFormsModule } from '@angular/forms'
import { WA_LOCAL_STORAGE } from '@ng-web-apis/common'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TuiAnimated, TuiValueChanges } from '@taiga-ui/cdk'
import {
  TUI_DARK_MODE,
  TUI_DARK_MODE_KEY,
  TuiAppearance,
  TuiButton,
  TuiDataList,
  TuiFilterByInputPipe,
  TuiIcon,
  TuiLabel,
  TuiNotification,
  TuiRadio,
  TuiTextfield,
  tuiTextfieldOptionsProvider,
  TuiTitle,
} from '@taiga-ui/core'
import { NgDompurifyPipe } from '@taiga-ui/dompurify'
import {
  TuiAccordion,
  TuiChevron,
  TuiComboBox,
  TUI_CONFIRM,
  TuiDataListWrapper,
  TuiSelect,
} from '@taiga-ui/kit'
import { TuiElasticContainer, TuiHeader } from '@taiga-ui/layout'
import { filter } from 'rxjs'
import { Footer } from 'src/app/components/footer'
import { Form } from 'src/app/components/form'
import { MarkdownPipe } from '@start9labs/shared'
import { ActionService } from 'src/app/services/action.service'
import {
  ApiService,
  RemoteAccess,
  Theme,
} from 'src/app/services/api/api.service'
import { SystemService } from 'src/app/services/system.service'
import { UPDATE_PROGRESS_DIALOG } from './update-progress-dialog'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { i18nService } from 'src/app/i18n/i18n.service'
import { Language, LANGUAGES } from 'src/app/utils/languages'
import { GIT_HASH } from 'src/app/utils/workspace-config'
import { getTimezoneLabel } from 'src/app/utils/timezones'

const THEMES: Theme[] = ['system', 'dark', 'light']

@Component({
  template: `
    @if (system.updateAvailable()) {
      <tui-accordion tuiAppearance="positive" [style.border-radius.rem]="1">
        <button tuiAccordion appearance="">
          <tui-icon icon="@tui.rocket" />
          <header tuiHeader="h6">
            <h2 tuiTitle>v{{ latestVersion() }} {{ 'released!' | i18n }}</h2>
          </header>
        </button>
        <tui-expand>
          @for (v of system.newerVersions(); track v.version) {
            <h2>v{{ v.version }}</h2>
            <div [innerHTML]="v.releaseNotes | markdown | dompurify"></div>
          }
          <div [style.margin-top.rem]="1">
            <button
              tuiButton
              appearance="positive"
              type="button"
              [disabled]="system.updating()"
              (click)="onUpdate()"
            >
              {{ 'Update now' | i18n }}
            </button>
          </div>
        </tui-expand>
      </tui-accordion>
    }
    <form
      [formGroup]="form"
      [formLoading]="false"
      (reset.prevent)="onCancel()"
      (ngSubmit)="onSubmit()"
    >
      <fieldset>
        <legend>{{ 'Preferences' | i18n }}</legend>
        <section>
          <tui-textfield tuiChevron [stringify]="stringifyTheme">
            <label tuiLabel>{{ 'Theme' | i18n }}</label>
            <input
              tuiSelect
              formControlName="theme"
              (tuiValueChanges)="onTheme($event)"
            />
            <tui-data-list-wrapper *tuiDropdown [items]="themes" />
          </tui-textfield>
          <tui-textfield tuiChevron [stringify]="stringifyLanguage">
            <label tuiLabel>{{ 'Language' | i18n }}</label>
            <input
              tuiSelect
              formControlName="language"
              (tuiValueChanges)="onLanguage($event)"
            />
            <tui-data-list *tuiDropdown>
              @for (lang of languages; track lang.posix) {
                <button tuiOption [value]="lang.posix">
                  <span tuiTitle>
                    {{ lang.nativeName }}
                    <span tuiSubtitle>{{ lang.name | i18n }}</span>
                  </span>
                </button>
              }
            </tui-data-list>
          </tui-textfield>
        </section>
      </fieldset>
      <tui-textfield tuiChevron [stringify]="stringifyTimezone">
        <label tuiLabel>{{ 'Timezone' | i18n }}</label>
        <input tuiComboBox formControlName="timezone" />
        <tui-data-list-wrapper
          *tuiDropdown
          [items]="timezones() | tuiFilterByInput"
        />
      </tui-textfield>
      <fieldset>
        <legend>{{ 'Remote Access' | i18n }}</legend>
        @for (value of ['default', 'never', 'always']; track $index) {
          <label tuiLabel>
            <input
              type="radio"
              tuiRadio
              formControlName="remote"
              [value]="value"
            />
            {{ $index ? (value | i18n) : ('When behind NAT (Default)' | i18n) }}
          </label>
        }
      </fieldset>
      <tui-elastic-container>
        @if (form.value.remote === 'always') {
          <div tuiAnimated tuiNotification appearance="warning">
            {{
              'This setting is not recommended as your router will be exposed to the internet'
                | i18n
            }}
          </div>
        }
      </tui-elastic-container>
      <fieldset>
        <legend>{{ 'Security' | i18n }}</legend>
        <section class="g-secondary">
          {{
            'Download your Root CA to trust HTTPS connections from additional devices.'
              | i18n
          }}
        </section>
      </fieldset>
      <a
        tuiButton
        size="s"
        iconEnd="@tui.download"
        href="/static/root-ca.crt"
        download="startwrt-ca.crt"
      >
        {{ 'Download Root CA' | i18n }}
      </a>
      <fieldset>
        <legend>{{ 'About' | i18n }}</legend>
        <dl>
          <dt class="g-secondary">{{ 'Version' | i18n }}</dt>
          <dd>{{ system.info()?.version || '—' }}</dd>
          <dt class="g-secondary">{{ 'Build' | i18n }}</dt>
          <dd>
            <code [title]="gitHash || ''">{{ shortGitHash() }}</code>
          </dd>
        </dl>
      </fieldset>
      <footer appFooter></footer>
    </form>
  `,
  styles: `
    tui-expand {
      font: var(--tui-typography-body-s);

      ::ng-deep {
        h1,
        h2,
        h3,
        h4 {
          margin: 0.75rem 0 0.25rem;
          font: var(--tui-typography-body-s);
          font-weight: bold;

          &:first-child {
            margin-top: 0;
          }
        }

        ul {
          margin: 0;
          padding-left: 1.25rem;
        }

        li {
          margin-bottom: 0.25rem;
        }

        p {
          margin: 0.5rem 0;

          &:first-child {
            margin-top: 0;
          }
        }
      }
    }

    :host {
      max-width: 50rem;

      fieldset {
        display: flex;
      }

      label {
        text-transform: capitalize;
      }

      tui-textfield {
        max-width: 25rem;
      }

      dl {
        display: grid;
        grid-template-columns: max-content 1fr;
        gap: 0.25rem 1rem;
        margin: 0;
      }
    }
  `,
  host: { class: 'g-page' },
  providers: [tuiTextfieldOptionsProvider({ cleaner: signal(false) })],
  imports: [
    ReactiveFormsModule,
    Form,
    Footer,
    TuiHeader,
    TuiTitle,
    TuiButton,
    TuiDataList,
    TuiIcon,
    TuiLabel,
    TuiNotification,
    TuiAppearance,
    TuiTextfield,
    TuiChevron,
    TuiRadio,
    TuiSelect,
    TuiComboBox,
    TuiFilterByInputPipe,
    TuiDataListWrapper,
    TuiValueChanges,
    TuiAccordion,
    MarkdownPipe,
    NgDompurifyPipe,
    TuiElasticContainer,
    TuiAnimated,
    i18nPipe,
  ],
})
export default class General {
  private readonly api = inject(ApiService)
  private readonly actions = inject(ActionService)
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly mode = inject(TUI_DARK_MODE)
  private readonly i18nService = inject(i18nService)
  private readonly i18n = inject(i18nPipe)

  protected readonly system = inject(SystemService)
  protected readonly gitHash = inject(GIT_HASH)
  protected readonly languages = LANGUAGES

  protected readonly shortGitHash = computed(() => {
    const hash = this.gitHash
    if (!hash) return 'unknown'
    // Shorten a leading 40-char hex hash to 12 chars while keeping any trailing
    // marker (e.g. "-modified" on a dirty build). Non-hash values (e.g. the
    // "@branch" GIT_BRANCH_AS_HASH dev stamp) don't match and pass through.
    return hash.replace(/^([0-9a-f]{12})[0-9a-f]{28}/, '$1')
  })

  protected readonly latestVersion = computed(() => {
    const versions = this.system.newerVersions()
    return versions.length ? versions[versions.length - 1].version : null
  })

  protected readonly form = inject(NonNullableFormBuilder).group({
    theme: (!inject(WA_LOCAL_STORAGE)?.getItem(inject(TUI_DARK_MODE_KEY))
      ? 'system'
      : this.mode()
        ? 'dark'
        : 'light') as Theme,
    language: 'en_US' as Language,
    remote: 'default',
    timezone: 'UTC',
  })

  constructor() {
    // Populate the dropdown from the device's authoritative zone list.
    this.api.getTimezones().then(zones => this.timezones.set(zones))

    effect(() => {
      const info = this.system.info()
      if (info && this.form.pristine) {
        this.form.reset({
          theme: info.theme,
          language: info.language as Language,
          remote: info.remoteAccess ?? 'default',
          // Show the actual device zone; default to UTC when unset (mirrors
          // LuCI). No browser fallback — that masked an unset device as set.
          timezone: (info.timezone || 'UTC').replaceAll(' ', '_'),
        })
      }
    })

    // Theme and language are previewed live on selection (see onTheme/onLanguage).
    // If the user leaves the page without saving, revert the preview to the saved
    // settings — the source of truth. A saved change is pristine, so this is a no-op.
    inject(DestroyRef).onDestroy(() => {
      if (this.form.pristine) return
      const info = this.system.info()
      if (info) {
        this.onTheme(info.theme)
        this.i18nService.setLangLocal(info.language as Language)
      }
    })
  }

  protected readonly themes = THEMES
  protected readonly timezones = signal<string[]>(['UTC'])

  protected readonly stringifyTheme = (t: Theme): string =>
    t ? this.i18n.transform(t[0].toUpperCase() + t.slice(1)) : ''

  protected readonly stringifyLanguage = (posix: Language): string =>
    LANGUAGES.find(l => l.posix === posix)?.nativeName || posix

  protected readonly stringifyTimezone = getTimezoneLabel

  // Live-preview the language on selection; the choice is persisted on submit
  // (the setPreferences call already carries `language`) or reverted on leave.
  protected onLanguage(language: Language): void {
    this.i18nService.setLangLocal(language)
  }

  // Live-preview the theme on selection; persisted on submit or reverted on leave.
  protected onTheme(theme: Theme): void {
    if (theme === 'system') {
      this.mode.reset()
    } else {
      this.mode.set(theme === 'dark')
    }
  }

  protected onCancel(): void {
    const info = this.system.info()
    if (info) {
      this.form.reset({
        theme: info.theme,
        language: info.language as Language,
        remote: info.remoteAccess ?? 'default',
        timezone: (info.timezone || 'UTC').replaceAll(' ', '_'),
      })
      this.onTheme(info.theme)
      this.i18nService.setLangLocal(info.language as Language)
    }
  }

  async onSubmit(): Promise<void> {
    await this.actions.run(
      async () => {
        await this.api.setPreferences({
          theme: this.form.value.theme!,
          language: this.form.value.language,
          remoteAccess: this.form.value.remote as RemoteAccess,
        })

        await this.api.setTimezone({ timezone: this.form.value.timezone! })

        await this.system.refresh()
        this.form.markAsPristine()
      },
      {
        fail: this.i18n.transform('Failed to save preferences'),
        success: this.i18n.transform('Preferences saved'),
      },
    )
  }

  protected onUpdate(): void {
    const latest = this.system.newerVersions().at(-1)
    if (!latest) return

    this.dialogs
      .open(TUI_CONFIRM, {
        label: this.i18n.transform('Start System Update?'),
        data: {
          content: `${this.i18n.transform('Updating to')} v${latest.version} ${this.i18n.transform('will restart your router. This may take several minutes and your network will be temporarily unavailable.')}`,
          yes: this.i18n.transform('Update Now'),
          no: this.i18n.transform('Cancel'),
        },
      })
      .pipe(filter(Boolean))
      .subscribe(() => this.doUpdate(latest.version))
  }

  private doUpdate(version: string): void {
    this.dialogs
      .open(UPDATE_PROGRESS_DIALOG, {
        label: this.i18n.transform('System Update'),
        data: version,
        closable: false,
        dismissible: false,
      })
      .subscribe()
  }
}
