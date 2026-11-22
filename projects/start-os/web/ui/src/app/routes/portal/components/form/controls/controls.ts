import { PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { FormAcknowledgementComponent } from './acknowledgement.component'
import { FormColorComponent } from './color.component'
import { FormDatetimeComponent } from './datetime.component'
import { FormFileComponent } from './file.component'
import { FormMultiselectComponent } from './multiselect.component'
import { FormNumberComponent } from './number.component'
import { FormSelectComponent } from './select.component'
import { FormTriStateComponent } from './tristate.component'
import { FormTextComponent } from './text.component'
import { FormTextareaComponent } from './textarea.component'
import { FormToggleComponent } from './toggle.component'

export const CONTROLS = {
  acknowledgement: new PolymorpheusComponent(FormAcknowledgementComponent),
  color: new PolymorpheusComponent(FormColorComponent),
  datetime: new PolymorpheusComponent(FormDatetimeComponent),
  file: new PolymorpheusComponent(FormFileComponent),
  number: new PolymorpheusComponent(FormNumberComponent),
  select: new PolymorpheusComponent(FormSelectComponent),
  triState: new PolymorpheusComponent(FormTriStateComponent),
  multiselect: new PolymorpheusComponent(FormMultiselectComponent),
  text: new PolymorpheusComponent(FormTextComponent),
  textarea: new PolymorpheusComponent(FormTextareaComponent),
  toggle: new PolymorpheusComponent(FormToggleComponent),
}
