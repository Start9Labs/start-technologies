import { inject, Injectable } from '@angular/core'
import {
  DialogService,
  i18nKey,
  i18nPipe,
  TaskService,
} from '@start9labs/shared'
import { ISB } from '@start9labs/start-core'

import { FormComponent } from 'src/app/routes/portal/components/form.component'
import { FormDialogService } from 'src/app/services/form-dialog.service'
import { configBuilderToSpec } from 'src/app/utils/configBuilderToSpec'
import { cifsSource, cifsSpec, Creator, localSpec } from './create'
import { DriveVolumeForm } from './drive-form'
import {
  SourcePickerData,
  VOLUME_SOURCE_PICKER,
  VolumeSourceKind,
} from './source-picker'

@Injectable({ providedIn: 'root' })
export class AddSourceService {
  private readonly dialogs = inject(DialogService)
  private readonly formDialog = inject(FormDialogService)
  private readonly tasks = inject(TaskService)
  private readonly i18n = inject(i18nPipe)

  open(label: i18nKey, data: SourcePickerData, create: Creator) {
    this.dialogs
      .openComponent<VolumeSourceKind>(VOLUME_SOURCE_PICKER, {
        label,
        size: 'l',
        appearance: 'taiga wide',
        data,
      })
      .subscribe(kind => this.form(kind, create))
  }

  async form(kind: VolumeSourceKind, create: Creator) {
    switch (kind) {
      case 'local':
        return this.simple(
          'Add local folder',
          localSpec(this.i18n),
          ({ name }) => ({ name, source: { kind: 'local' } }),
          create,
        )
      case 'cifs':
        return this.simple(
          'Add CIFS share',
          cifsSpec(this.i18n),
          value => ({ name: value.hostname, source: cifsSource(value) }),
          create,
        )
      case 'disk':
        return this.formDialog.open(DriveVolumeForm, {
          label: 'Add external drive',
          data: { create },
        })
    }
  }

  private async simple<S>(
    label: i18nKey,
    spec: ISB.InputSpec<S>,
    params: (value: S) => Parameters<Creator>[0],
    create: Creator,
  ) {
    this.formDialog.open(FormComponent, {
      label,
      data: {
        spec: await configBuilderToSpec(spec),
        buttons: [
          {
            text: this.i18n.transform('Save'),
            handler: async (value: S) =>
              this.tasks.run(() => create(params(value)), 'Saving'),
          },
        ],
      },
    })
  }
}
