import { AsyncPipe } from '@angular/common'
import { Component, inject, INJECTOR } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { FormsModule } from '@angular/forms'
import { RouterLink } from '@angular/router'
import { WA_WINDOW } from '@ng-web-apis/common'
import {
  DialogService,
  getAllKeyboardsSorted,
  getKeyboardName,
  i18nKey,
  i18nPipe,
  i18nService,
  Keyboard,
  KeyboardLayout,
  Language,
  LANGUAGE_TO_CODE,
  LANGUAGES,
  TaskService,
} from '@start9labs/shared'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TuiAnimated } from '@taiga-ui/cdk'
import {
  TuiAppearance,
  TuiButton,
  TuiCell,
  tuiCellOptionsProvider,
  TuiIcon,
  TuiInput,
  TuiTitle,
} from '@taiga-ui/core'
import {
  TuiBadge,
  TuiBadgeNotification,
  TuiButtonLoading,
  TuiButtonSelect,
  TuiDataListWrapper,
} from '@taiga-ui/kit'
import { PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { PatchDB } from 'patch-db-client'
import { filter } from 'rxjs'
import { ABOUT } from 'src/app/routes/portal/components/header/about.component'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { ConfigService } from 'src/app/services/config.service'
import { OSService } from 'src/app/services/os.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { TitleDirective } from 'src/app/services/title.service'
import { KeyboardSelectComponent } from './keyboard-select.component'
import { ServerNameDialog } from './server-name.dialog'
import { SnakeDirective } from './snake.directive'
import { UPDATE } from './update.component'

@Component({
  template: `
    <ng-container *title>
      <a routerLink=".." tuiIconButton iconStart="@tui.arrow-left">
        {{ 'Back' | i18n }}
      </a>
      {{ 'General Settings' | i18n }}
    </ng-container>
    @if (server(); as server) {
      <div tuiCell tuiAppearance="outline-grayscale">
        <tui-icon icon="@tui.info" />
        <span tuiTitle>
          <strong>{{ 'About this server' | i18n }}</strong>
          <span tuiSubtitle>
            {{ 'Version, Root CA, and more' | i18n }}
          </span>
        </span>
        <button tuiButton (click)="about()">
          {{ 'Details' | i18n }}
        </button>
      </div>
      <div tuiCell tuiAppearance="outline-grayscale">
        <tui-icon icon="@tui.zap" (click)="count = count + 1" />
        <span tuiTitle>
          <strong>
            {{ 'Software Update' | i18n }}
            @if (os.showUpdate$ | async) {
              <tui-badge-notification>
                {{ 'Update available' | i18n }}
              </tui-badge-notification>
            }
          </strong>
        </span>
        <button
          tuiButton
          appearance="accent"
          iconStart="@tui.refresh-cw"
          [disabled]="os.updatingOrBackingUp$ | async"
          (click)="onUpdate()"
        >
          @if (os.showUpdate$ | async) {
            {{ 'Update' | i18n }}
          } @else {
            {{ 'Check for updates' | i18n }}
          }
        </button>
      </div>
      <div tuiCell tuiAppearance="outline-grayscale">
        <tui-icon icon="@tui.server" />
        <span tuiTitle>
          <strong>{{ 'Server Name' | i18n }}</strong>
          <span tuiSubtitle>{{ server.hostname }}.local</span>
        </span>
        <button tuiButton (click)="onName()">
          {{ 'Change' | i18n }}
        </button>
      </div>
      <div tuiCell tuiAppearance="outline-grayscale">
        <tui-icon icon="@tui.languages" />
        <span tuiTitle>
          <strong>{{ 'Language' | i18n }}</strong>
          <span tuiSubtitle>
            {{ currentLanguage?.nativeName }}
          </span>
        </span>
        <button
          tuiButtonSelect
          tuiButton
          [loading]="i18nService.loading()"
          [ngModel]="currentLanguage"
          (ngModelChange)="onLanguageChange($event)"
        >
          {{ 'Change' | i18n }}
          <tui-data-list-wrapper
            *tuiDropdown
            size="l"
            [items]="languages"
            [itemContent]="languageContent"
          />
        </button>
        <ng-template #languageContent let-item>
          {{ item.nativeName }}
        </ng-template>
      </div>
      <div tuiCell tuiAppearance="outline-grayscale">
        <tui-icon icon="@tui.monitor" />
        <span tuiTitle>
          <strong>
            {{ 'Kiosk Mode' | i18n }}
            <span
              tuiBadge
              size="m"
              [appearance]="server.kiosk ? 'warning' : 'primary-grayscale'"
            >
              {{ server.kiosk ? ('Enabled' | i18n) : ('Disabled' | i18n) }}
            </span>
          </strong>
          <span tuiSubtitle [class.warning-text]="server.kiosk">
            @if (server.kiosk === null) {
              {{ 'Kiosk Mode is unavailable on this device' | i18n }}
            } @else {
              {{
                server.kiosk
                  ? ('Disable Kiosk Mode unless you need to attach a monitor'
                    | i18n)
                  : ('Enable Kiosk Mode if you need to attach a monitor' | i18n)
              }}
            }
          </span>
          @if (server.kiosk !== null && server.keyboard?.layout; as layout) {
            <span tuiSubtitle class="keyboard-info">
              <tui-icon icon="@tui.keyboard" />
              {{ getKeyboardName(layout) }}
              <button
                tuiIconButton
                appearance="icon"
                iconStart="@tui.pencil"
                size="xs"
                (click)="onChangeKeyboard()"
              ></button>
            </span>
          }
        </span>
        @if (server.kiosk !== null) {
          <button tuiButton appearance="primary" (click)="toggleKiosk()">
            {{ server.kiosk ? ('Disable' | i18n) : ('Enable' | i18n) }}
          </button>
        }
      </div>
      @if (count > 4) {
        <div tuiCell tuiAppearance="outline-grayscale" tuiAnimated>
          <tui-icon icon="@tui.briefcase-medical" />
          <span tuiTitle>
            <strong>{{ 'Disk Repair' | i18n }}</strong>
            <span tuiSubtitle>{{ 'Attempt automatic repair' | i18n }}</span>
          </span>
          <button
            tuiButton
            appearance="primary-destructive"
            (click)="onRepair()"
          >
            {{ 'Repair' | i18n }}
          </button>
        </div>
      }
      <img
        [snake]="score() || 0"
        class="snake"
        alt="Play Snake"
        src="assets/img/icons/snake.png"
      />
    }
  `,
  styles: `
    :host {
      max-inline-size: 40rem;
    }

    .snake {
      width: 1rem;
      opacity: 0.2;
      cursor: pointer;

      &:hover {
        opacity: 1;
      }
    }

    strong {
      line-height: 1.25rem;
    }

    [tuiCell] {
      background: var(--tui-background-neutral-1);
    }

    [tuiAnimated].tui-enter,
    [tuiAnimated].tui-leave {
      animation-name: tuiFade, tuiScale;
    }

    .keyboard-info {
      display: flex;
      align-items: center;
      gap: 0.25rem;

      tui-icon {
        font-size: 0.875rem;
      }
    }

    .warning-text,
    [tuiSubtitle].warning-text {
      color: var(--tui-status-warning) !important;
    }
  `,
  providers: [tuiCellOptionsProvider({ height: 'spacious' })],
  imports: [
    AsyncPipe,
    RouterLink,
    i18nPipe,
    TuiTitle,
    TuiCell,
    TuiAppearance,
    TuiButton,
    TuiIcon,
    TitleDirective,
    TuiButtonLoading,
    TuiButtonSelect,
    TuiDataListWrapper,
    TuiInput,
    FormsModule,
    SnakeDirective,
    TuiBadge,
    TuiBadgeNotification,
    TuiAnimated,
  ],
})
export default class SystemGeneralComponent {
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly tasks = inject(TaskService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly api = inject(ApiService)
  private readonly dialog = inject(DialogService)
  private readonly i18n = inject(i18nPipe)
  private readonly injector = inject(INJECTOR)
  private readonly win = inject(WA_WINDOW)
  private readonly config = inject(ConfigService)

  count = 0

  about() {
    this.dialog.openComponent(ABOUT, { label: 'About this server' }).subscribe()
  }

  readonly server = toSignal(this.patch.watch$('serverInfo'))
  readonly score = toSignal(this.patch.watch$('ui', 'snakeHighScore'))
  readonly os = inject(OSService)
  readonly i18nService = inject(i18nService)
  readonly languages = LANGUAGES

  get currentLanguage(): Language | undefined {
    return LANGUAGES.find(lang => lang.name === this.i18nService.lang)
  }

  onLanguageChange(language: Language) {
    this.i18nService.setLang(language.name)
  }

  // Expose shared utilities for template use
  readonly getKeyboardName = getKeyboardName

  /**
   * Open keyboard selection dialog to change keyboard layout
   */
  onChangeKeyboard() {
    const server = this.server()
    if (!server) return

    const keyboards = getAllKeyboardsSorted(LANGUAGE_TO_CODE[server.language])
    const currentLayout = (server.keyboard?.layout as KeyboardLayout) || null

    this.dialog
      .openComponent<Keyboard | null>(
        new PolymorpheusComponent(KeyboardSelectComponent, this.injector),
        {
          label: 'Select Keyboard Layout',
          size: 's',
          data: { keyboards, currentLayout },
        },
      )
      .pipe(filter((keyboard): keyboard is Keyboard => keyboard !== null))
      .subscribe(keyboard => {
        this.saveKeyboard(keyboard)
      })
  }

  private async saveKeyboard(keyboard: Keyboard) {
    this.tasks.run(
      async () =>
        await this.api.setKeyboard({
          layout: keyboard.layout,
          keymap: keyboard.keymap,
          model: null,
          variant: null,
          options: [],
        }),
      'Saving',
    )
  }

  onUpdate() {
    if (this.os.updateAvailable$.value) {
      this.update()
    } else {
      this.check()
    }
  }

  onName() {
    const server = this.server()
    if (!server) return

    this.dialog
      .openComponent<string | null>(
        new PolymorpheusComponent(ServerNameDialog, this.injector),
        {
          label: 'Server Name',
          size: 's',
          data: { hostname: server.hostname },
        },
      )
      .pipe(filter(Boolean))
      .subscribe(hostname => {
        if (this.config.accessType === 'mdns') {
          this.confirmNameChange(hostname)
        } else {
          this.saveName(hostname)
        }
      })
  }

  private confirmNameChange(hostname: string) {
    this.dialog
      .openConfirm({
        label: 'Warning',
        data: {
          content:
            'You are currently connected via your .local address. Changing the hostname will require you to switch to the new .local address. A server restart will also be needed.',
          yes: 'Save',
          no: 'Cancel',
        },
      })
      .pipe(filter(Boolean))
      .subscribe(() => this.saveName(hostname, true))
  }

  private async saveName(hostname: string, wasLocal = false) {
    const success = await this.tasks.run(
      async () => await this.api.setHostname({ hostname }),
      'Saving',
    )

    if (wasLocal && success) {
      this.dialog
        .openConfirm({
          label: 'Hostname Changed',
          data: {
            content:
              `${this.i18n.transform('Your server is now reachable at')} ${hostname}.local` as i18nKey,
            yes: 'Open new address',
            no: 'Dismiss',
          },
        })
        .pipe(filter(Boolean))
        .subscribe(() => this.win.open(`https://${hostname}.local`, '_blank'))
    }
  }

  async onRepair() {
    this.dialog
      .openConfirm({
        label: 'Warning',
        data: {
          content:
            'This action should only be executed if directed by a Start9 support specialist. We recommend backing up your device before preforming this action. If anything happens to the device during the reboot, such as losing power or unplugging the drive, the filesystem will be in an unrecoverable state. Please proceed with caution.',
          yes: 'Repair',
          no: 'Cancel',
        },
      })
      .pipe(filter(Boolean))
      .subscribe(() =>
        this.tasks.run(async () => {
          await this.api.repairDisk({})
          this.restart()
        }),
      )
  }

  async toggleKiosk() {
    const server = this.server()
    if (!server) return

    const kiosk = server.kiosk

    // If disabling kiosk, just disable it
    if (kiosk) {
      await this.disableKiosk()
      return
    }

    // Enabling kiosk - check if keyboard is already set
    if (server.keyboard) {
      // Keyboard already set, just enable kiosk
      await this.enableKiosk()
      return
    }

    // No keyboard set - prompt user to select from all keyboards
    const keyboards = getAllKeyboardsSorted(LANGUAGE_TO_CODE[server.language])
    this.promptKeyboardSelection(keyboards)
  }

  private promptKeyboardSelection(keyboards: Keyboard[]) {
    this.dialog
      .openComponent<Keyboard | null>(
        new PolymorpheusComponent(KeyboardSelectComponent, this.injector),
        {
          label: 'Select Keyboard Layout',
          size: 's',
          data: { keyboards, currentLayout: null },
        },
      )
      .pipe(filter((keyboard): keyboard is Keyboard => keyboard !== null))
      .subscribe(keyboard => {
        this.enableKioskWithKeyboard(keyboard)
      })
  }

  private async enableKiosk() {
    this.tasks.run(async () => await this.api.toggleKiosk(true), 'Enabling')
  }

  private async enableKioskWithKeyboard(keyboard: Keyboard) {
    this.tasks.run(async () => {
      await this.api.setKeyboard({
        layout: keyboard.layout,
        keymap: keyboard.keymap,
        model: null,
        variant: null,
        options: [],
      })
      await this.api.toggleKiosk(true)
    }, 'Enabling')
  }

  private async disableKiosk() {
    this.tasks.run(async () => await this.api.toggleKiosk(false), 'Disabling')
  }

  private update() {
    this.dialogs
      .open(UPDATE, { data: { currentVersion: this.server()?.version } })
      .subscribe()
  }

  private async check(): Promise<void> {
    const success = await this.tasks.run(
      async () => await this.os.loadOS(),
      'Checking for updates',
    )

    if (this.os.updateAvailable$.value) {
      this.update()
    } else if (success) {
      this.dialog
        .openAlert('You are on the latest version of StartOS.', {
          label: 'Up to date!',
        })
        .subscribe()
    }
  }

  private async restart() {
    this.tasks.run(
      async () => await this.api.restartServer({}),
      'Beginning restart',
    )
  }
}
