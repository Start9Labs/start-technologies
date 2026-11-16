import { Pipe, PipeTransform } from '@angular/core'

import { renderDisplayAs } from 'src/app/utils/display-as'

@Pipe({
  name: 'mustache',
})
export class MustachePipe implements PipeTransform {
  transform(value: unknown, displayAs: string): string {
    return displayAs && renderDisplayAs(displayAs, value)
  }
}
