import { Component, inject, signal } from '@angular/core'
import {
  AbstractControl,
  FormControl,
  FormGroup,
  ReactiveFormsModule,
  ValidatorFn,
  Validators,
} from '@angular/forms'
import { Router } from '@angular/router'
import { WA_IS_MOBILE } from '@ng-web-apis/platform'
import {
  DialogService,
  DiskInfo,
  ErrorService,
  i18nKey,
  i18nPipe,
  TaskService,
  toGuid,
} from '@start9labs/shared'
import { TuiMapperPipe, TuiValidator } from '@taiga-ui/cdk'
import {
  TUI_VALIDATION_ERRORS,
  TuiButton,
  TuiError,
  TuiIcon,
  TuiLoader,
  TuiNotification,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiDataListWrapper, TuiSelect, TuiTooltip } from '@taiga-ui/kit'
import { TuiCardLarge, TuiForm, TuiHeader } from '@taiga-ui/layout'
import { distinctUntilChanged, filter, Subscription } from 'rxjs'
import { PRESERVE_OVERWRITE } from '../components/preserve-overwrite.dialog'
import { ApiService } from '../services/api.service'
import { StateService } from '../services/state.service'

@Component({
  template: `
    @if (!shuttingDown()) {
      @if (loading()) {
        <section tuiCardLarge="compact">
          <header tuiHeader>
            <h2 tuiTitle>{{ 'Select Drives' | i18n }}</h2>
          </header>
          <tui-loader />
        </section>
      } @else if (drives().length === 0) {
        <section tuiCardLarge="compact">
          <header tuiHeader>
            <h2 tuiTitle>{{ 'Select Drives' | i18n }}</h2>
          </header>
          <p tuiNotification size="m" appearance="warning">
            {{
              'No drives found. Please connect a drive and click Refresh.'
                | i18n
            }}
          </p>
          <footer>
            <button tuiButton appearance="secondary" (click)="refresh()">
              {{ 'Refresh' | i18n }}
            </button>
          </footer>
        </section>
      } @else {
        <form tuiCardLarge="compact" tuiForm [formGroup]="form">
          <header tuiHeader>
            <h2 tuiTitle>{{ 'Select Drives' | i18n }}</h2>
          </header>

          <tui-textfield [stringify]="stringify">
            <label tuiLabel>{{ 'OS Drive' | i18n }}</label>
            @if (mobile) {
              <select
                tuiSelect
                formControlName="osDrive"
                [items]="drives()"
              ></select>
            } @else {
              <input tuiSelect formControlName="osDrive" />
            }
            @if (!mobile) {
              <tui-data-list-wrapper
                *tuiDropdown
                [items]="drives()"
                [itemContent]="driveContent"
              />
            }
            <tui-icon [tuiTooltip]="osDriveTooltip" />
          </tui-textfield>
          @if (form.controls.osDrive.touched && form.controls.osDrive.invalid) {
            <tui-error formControlName="osDrive" />
          }

          <tui-textfield [stringify]="stringify">
            <label tuiLabel>{{ 'Data Drive' | i18n }}</label>
            @if (mobile) {
              <select
                tuiSelect
                formControlName="dataDrive"
                [items]="drives()"
                [tuiValidator]="
                  form.controls.osDrive.value | tuiMapper: dataValidator
                "
              ></select>
            } @else {
              <input
                tuiSelect
                formControlName="dataDrive"
                [tuiValidator]="
                  form.controls.osDrive.value | tuiMapper: dataValidator
                "
              />
            }
            @if (!mobile) {
              <tui-data-list-wrapper
                *tuiDropdown
                [items]="drives()"
                [itemContent]="driveContent"
              />
            }
            @if (preserveData() === true) {
              <tui-icon icon="@tui.database" class="g-positive" />
            }
            @if (preserveData() === false) {
              <tui-icon icon="@tui.database-zap" class="g-negative" />
            }
            <tui-icon [tuiTooltip]="dataDriveTooltip" />
          </tui-textfield>
          @if (
            form.controls.dataDrive.touched && form.controls.dataDrive.invalid
          ) {
            <tui-error formControlName="dataDrive" />
          }

          <ng-template #driveContent let-drive>
            <span tuiTitle>
              {{ driveName(drive) }}
              <span tuiSubtitle>
                {{ formatCapacity(drive.capacity) }} · {{ drive.logicalname }}
              </span>
            </span>
          </ng-template>

          <footer>
            <button tuiButton [disabled]="form.invalid" (click)="continue()">
              {{ 'Continue' | i18n }}
            </button>
          </footer>
        </form>
      }
    }
  `,
  styles: `
    tui-icon:not([tuiTooltip]) {
      pointer-events: none;
    }
  `,
  imports: [
    ReactiveFormsModule,
    TuiCardLarge,
    TuiForm,
    TuiButton,
    TuiError,
    TuiIcon,
    TuiLoader,
    TuiNotification,
    TuiSelect,
    TuiDataListWrapper,
    TuiTooltip,
    TuiValidator,
    TuiMapperPipe,
    TuiHeader,
    TuiTitle,
    i18nPipe,
  ],
  providers: [
    {
      provide: TUI_VALIDATION_ERRORS,
      useFactory: () => {
        const i18n = inject(i18nPipe)
        return {
          required: i18n.transform('Required'),
        }
      },
    },
  ],
  host: { '(document:keydown)': 'onKeydown($event)' },
})
export default class DrivesPage {
  private readonly api = inject(ApiService)
  private readonly router = inject(Router)
  private readonly dialogs = inject(DialogService)
  private readonly tasks = inject(TaskService)
  private readonly errorService = inject(ErrorService)
  private readonly stateService = inject(StateService)
  private readonly i18n = inject(i18nPipe)

  protected readonly mobile = inject(WA_IS_MOBILE)

  onKeydown(event: KeyboardEvent) {
    if (event.ctrlKey && event.shiftKey && event.key === 'X') {
      event.preventDefault()
      this.shutdownServer()
    }
  }

  readonly osDriveTooltip = this.i18n.transform(
    'The drive where the StartOS operating system will be installed. Minimum 18 GB.',
  )
  readonly dataDriveTooltip = this.i18n.transform(
    'The drive where your StartOS data (services, settings, etc.) will be stored. This can be the same as the OS drive or a separate drive. Minimum 20 GB, or 38 GB if using a single drive for both OS and data.',
  )

  private readonly MIN_OS = 18 * 2 ** 30 // 18 GiB
  private readonly MIN_DATA = 20 * 2 ** 30 // 20 GiB
  private readonly MIN_BOTH = 38 * 2 ** 30 // 38 GiB

  private readonly osCapacityValidator: ValidatorFn = ({
    value,
  }: AbstractControl) => {
    if (!value) return null
    return value.capacity < this.MIN_OS
      ? {
          tooSmallOs: this.i18n.transform('OS drive must be at least 18 GB'),
        }
      : null
  }

  readonly form = new FormGroup({
    osDrive: new FormControl<DiskInfo | null>(null, [
      Validators.required,
      this.osCapacityValidator,
    ]),
    dataDrive: new FormControl<DiskInfo | null>(null, [Validators.required]),
  })

  readonly dataValidator =
    (osDrive: DiskInfo | null): ValidatorFn =>
    ({ value }: AbstractControl) => {
      if (!value) return null
      const sameAsOs = osDrive && value.logicalname === osDrive.logicalname
      const min = sameAsOs ? this.MIN_BOTH : this.MIN_DATA
      if (value.capacity < min) {
        return sameAsOs
          ? {
              tooSmallBoth: this.i18n.transform(
                'OS + data combined require at least 38 GB',
              ),
            }
          : {
              tooSmallData: this.i18n.transform(
                'Data drive must be at least 20 GB',
              ),
            }
      }
      return null
    }

  readonly drives = signal<DiskInfo[]>([])
  readonly loading = signal(true)
  readonly shuttingDown = signal(false)
  private dialogSub?: Subscription
  readonly preserveData = signal<boolean | null>(null)

  readonly driveName = (drive: DiskInfo): string =>
    [drive.vendor, drive.model].filter(Boolean).join(' ') ||
    this.i18n.transform('Unknown Drive')

  readonly stringify = (drive: DiskInfo | null) =>
    drive ? this.driveName(drive) : ''

  formatCapacity(bytes: number): string {
    const gb = bytes / 1e9
    if (gb >= 1000) {
      return `${(gb / 1000).toFixed(1)} TB`
    }
    return `${gb.toFixed(0)} GB`
  }

  async ngOnInit() {
    await this.loadDrives()

    // Pre-installed device: fix the OS drive to the disk the OS booted from and
    // disable it. The user only selects a data drive; the backend provisions
    // only that drive and leaves the OS untouched.
    if (this.stateService.osDrive) {
      this.form.controls.osDrive.setValue({
        logicalname: this.stateService.osDrive,
        vendor: null,
        model: this.stateService.osDrive,
        partitions: [],
        capacity: 0,
        guid: null,
        filesystem: null,
      })
      this.form.controls.osDrive.disable()
    }

    this.form.controls.osDrive.valueChanges.subscribe(drive => {
      if (drive) {
        this.form.controls.osDrive.markAsTouched()
        // A "Preserve" choice is only valid against the OS drive it was made
        // for: re-offer it when the new combination cannot keep the data.
        const dataDrive = this.form.controls.dataDrive.value
        if (
          this.preserveData() &&
          dataDrive &&
          this.preserveBlockedReason(dataDrive)
        ) {
          this.showPreserveOverwriteDialog()
        }
      }
    })

    this.form.controls.dataDrive.valueChanges
      .pipe(distinctUntilChanged())
      .subscribe(drive => {
        this.preserveData.set(null)
        if (drive) {
          this.form.controls.dataDrive.markAsTouched()
          if (toGuid(drive)) {
            this.showPreserveOverwriteDialog()
          }
        }
      })
  }

  async refresh() {
    this.loading.set(true)
    this.form.reset()
    this.preserveData.set(null)
    await this.loadDrives()
  }

  continue() {
    const osDrive = this.form.controls.osDrive.value
    const dataDrive = this.form.controls.dataDrive.value
    if (!osDrive || !dataDrive) return

    // Pre-installed: OS drive is fixed and never touched; warn only about the
    // data drive being overwritten (unless we're preserving existing data).
    if (this.stateService.osDrive) {
      if (toGuid(dataDrive) && this.preserveData()) {
        this.installOs(false)
      } else {
        this.dialogs
          .openConfirm({
            label: 'Warning',
            data: {
              content:
                `<p class="g-negative">${this.i18n.transform('Data on this drive will be overwritten.')}</p>` as i18nKey,
              yes: 'Continue',
              no: 'Cancel',
            },
          })
          .pipe(filter(Boolean))
          .subscribe(() => {
            this.installOs(true)
          })
      }
      return
    }

    const sameDevice = osDrive.logicalname === dataDrive.logicalname
    const dataHasStartOS = !!toGuid(dataDrive)

    // Scenario 1: Same drive, has StartOS data, preserving → no warning
    if (sameDevice && dataHasStartOS && this.preserveData()) {
      this.installOs(false)
      return
    }

    // Scenario 2: Different drives, preserving data → warn OS only
    if (!sameDevice && this.preserveData()) {
      this.showOsDriveWarning()
      return
    }

    // Scenario 3: All other cases → warn about overwriting
    this.showFullWarning(sameDevice)
  }

  private readonly isStartOsPoolGuid = (guid: string | null): boolean =>
    !!guid && (guid.startsWith('EMBASSY_') || guid.startsWith('STARTOS_'))

  // Mirror of the backend plan_data_drive (os_install/mod.rs): why a "Preserve"
  // selection cannot keep the data, or null when the pool can be attached. An
  // unselected OS drive never blocks — the choice is re-validated on selection.
  private preserveBlockedReason(dataDrive: DiskInfo): i18nKey | null {
    const osDrive = this.stateService.osDrive
      ? null
      : this.form.controls.osDrive.value?.logicalname
    const wholeDiskPool = this.isStartOsPoolGuid(dataDrive.guid)
    const partitionPool = dataDrive.partitions.some(p =>
      this.isStartOsPoolGuid(p.guid),
    )
    const sameDrive = osDrive === dataDrive.logicalname

    if (wholeDiskPool) {
      return sameDrive
        ? 'The StartOS data on the selected data drive spans the entire drive, so the OS cannot be installed to the same drive without erasing it. To preserve your data, select a different OS drive. To erase it, choose "Overwrite".'
        : null
    }

    if (partitionPool) {
      if (sameDrive || osDrive === undefined) return null

      return osDrive
        ? 'The StartOS data on the selected data drive is stored on a partition alongside an older OS installation, and cannot be preserved while the OS is installed to a different drive. To keep your data, select this same drive for both the OS drive and the data drive. To erase it instead, choose "Overwrite".'
        : 'The StartOS data on the selected data drive is stored on a partition alongside an older OS installation, and cannot be preserved on this device. To erase the drive and start fresh, choose "Overwrite".'
    }

    return 'No StartOS data was found on the selected data drive. If your data is on a different drive, select that drive instead. To erase this drive and start fresh, choose "Overwrite".'
  }

  private showPreserveOverwriteDialog() {
    let selectionMade = false
    const drive = this.form.controls.dataDrive.value
    if (!drive) return

    const filesystem =
      drive.filesystem || drive.partitions.find(p => p.guid)?.filesystem || null
    const isExt4 = filesystem === 'ext2'

    this.dialogs
      .openComponent<boolean>(PRESERVE_OVERWRITE, {
        data: { isExt4, blockedReason: this.preserveBlockedReason(drive) },
      })
      .subscribe({
        next: preserve => {
          selectionMade = true
          this.preserveData.set(preserve)
        },
        complete: () => {
          if (!selectionMade) {
            // Dialog was dismissed without selection - clear the data drive
            this.form.controls.dataDrive.reset()
            this.preserveData.set(null)
          }
        },
      })
  }

  private showOsDriveWarning() {
    this.dialogs
      .openConfirm({
        label: 'Warning',
        data: {
          content: `<ul>
<li class="g-negative">${this.i18n.transform('Data on the OS drive may be overwritten.')}</li>
<li class="g-positive">${this.i18n.transform('your StartOS data on the data drive will be preserved.')}</li>
</ul>` as i18nKey,
          yes: 'Continue',
          no: 'Cancel',
        },
      })
      .pipe(filter(Boolean))
      .subscribe(() => {
        this.installOs(false)
      })
  }

  private showFullWarning(sameDevice: boolean) {
    const message = sameDevice
      ? `<p class="g-negative">${this.i18n.transform('Data on this drive will be overwritten.')}</p>`
      : `<p class="g-negative">${this.i18n.transform('Data on both drives will be overwritten.')}</p>`

    this.dialogs
      .openConfirm({
        label: 'Warning',
        data: {
          content: message as i18nKey,
          yes: 'Continue',
          no: 'Cancel',
        },
      })
      .pipe(filter(Boolean))
      .subscribe(() => {
        this.installOs(true)
      })
  }

  private async installOs(wipe: boolean) {
    const osDrive = this.form.controls.osDrive.value!
    const dataDrive = this.form.controls.dataDrive.value!

    this.tasks.run(async () => {
      const result = await this.api.installOs({
        // Pre-installed: null OS drive tells the backend to skip the install
        // and only provision the data drive.
        osDrive: this.stateService.osDrive ? null : osDrive.logicalname,
        dataDrive: {
          logicalname: dataDrive.logicalname,
          wipe,
        },
      })

      this.stateService.dataDriveGuid = result.guid
      this.stateService.attach = result.attach
      this.stateService.mokEnrolled = result.mokEnrolled

      console.log('Ctrl+Shift+X to shutdown')

      // Show success dialog
      this.dialogSub = this.dialogs
        .openAlert('StartOS has been installed successfully.', {
          label: 'Installation Complete!',
          dismissible: false,
          closable: true,
          data: this.i18n.transform('Continue to Setup'),
        })
        .subscribe({
          complete: () => {
            this.navigateToNextStep(result.attach)
          },
        })
    }, 'Installing StartOS')
  }

  private async navigateToNextStep(attach: boolean) {
    if (attach) {
      this.stateService.setupType = 'attach'
      await this.router.navigate(['/password'])
    } else {
      await this.router.navigate(['/home'])
    }
  }

  private async shutdownServer() {
    this.dialogSub?.unsubscribe()

    this.tasks.run(async () => {
      await this.api.shutdown()
      this.shuttingDown.set(true)
    }, 'Beginning shutdown')
  }

  private async loadDrives() {
    try {
      this.drives.set((await this.api.getDisks()).filter(d => d.capacity > 0))
    } catch (e: any) {
      this.errorService.handleError(e)
    } finally {
      this.loading.set(false)
    }
  }
}
