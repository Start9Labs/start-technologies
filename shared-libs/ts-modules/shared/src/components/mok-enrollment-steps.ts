import { Component } from '@angular/core'
import { i18nPipe } from '../i18n/i18n.pipe'

@Component({
  selector: 'mok-enrollment-steps',
  imports: [i18nPipe],
  template: `
    <ol>
      <li>
        {{
          'At the next restart, press a key when prompted to enter MOK management.'
            | i18n
        }}
      </li>
      <li>
        {{
          'Select "Enroll MOK", review the certificate fingerprints, then select "Continue" and "Yes".'
            | i18n
        }}
      </li>
      <li><ng-content /></li>
      <li>{{ 'Select "Reboot" to finish firmware enrollment.' | i18n }}</li>
    </ol>
  `,
})
export class MokEnrollmentSteps {}
