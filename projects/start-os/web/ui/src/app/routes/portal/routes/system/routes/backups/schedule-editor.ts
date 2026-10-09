import { FormControl } from '@angular/forms'

import { BackupScheduleFormValue } from './scheduled-utils'

export abstract class BackupScheduleEditor implements BackupScheduleFormValue {
  abstract readonly form: {
    controls: {
      includeFuture: FormControl<boolean>
      keepAdditional: FormControl<boolean>
      password: FormControl<string>
      firstBackupNow: FormControl<boolean>
      capacityConfirmed: FormControl<boolean>
    }
  }
  frequency: BackupScheduleFormValue['frequency']
  minute: number
  hour: number
  weekday: number
  dayOfMonth: number
  timezone: string

  constructor(value: BackupScheduleFormValue) {
    this.frequency = value.frequency
    this.minute = value.minute
    this.hour = value.hour
    this.weekday = value.weekday
    this.dayOfMonth = value.dayOfMonth
    this.timezone = value.timezone
  }

  get includeFuture() {
    return this.form.controls.includeFuture.value
  }
  set includeFuture(value: boolean) {
    this.form.controls.includeFuture.setValue(value)
  }

  get keepAdditional() {
    return this.form.controls.keepAdditional.value
  }
  set keepAdditional(value: boolean) {
    this.form.controls.keepAdditional.setValue(value)
  }

  get password() {
    return this.form.controls.password.value
  }
  set password(value: string) {
    this.form.controls.password.setValue(value)
  }

  get firstBackupNow() {
    return this.form.controls.firstBackupNow.value
  }
  set firstBackupNow(value: boolean) {
    this.form.controls.firstBackupNow.setValue(value)
  }

  get capacityConfirmed() {
    return this.form.controls.capacityConfirmed.value
  }
  set capacityConfirmed(value: boolean) {
    this.form.controls.capacityConfirmed.setValue(value)
  }

  protected scheduleValue(): BackupScheduleFormValue {
    return {
      frequency: this.frequency,
      minute: this.minute,
      hour: this.hour,
      weekday: this.weekday,
      dayOfMonth: this.dayOfMonth,
      timezone: this.timezone,
    }
  }
}
