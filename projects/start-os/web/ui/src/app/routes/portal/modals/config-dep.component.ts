import { Component, inject, Input, OnInit } from '@angular/core'
import { TuiNotification } from '@taiga-ui/core'
import { getValueByPointer, Operation } from 'fast-json-patch'
import { i18nPipe, isObject } from '@start9labs/shared'
import { tuiIsNumber } from '@taiga-ui/cdk'

@Component({
  selector: 'task-info',
  template: `
    @if (diff.length) {
      <div tuiNotification>
        {{ 'The following modifications were made' | i18n }}:
        <ul>
          @for (d of diff; track d) {
            <li [innerHTML]="d"></li>
          }
        </ul>
      </div>
    }
  `,
  imports: [TuiNotification, i18nPipe],
})
export class TaskInfoComponent implements OnInit {
  @Input()
  originalValue: object = {}

  @Input()
  operations: Operation[] = []

  private readonly i18n = inject(i18nPipe)

  diff: string[] = []

  ngOnInit() {
    this.diff = this.operations.map(
      op => `${this.getPath(op)}: ${this.getMessage(op)}`,
    )
  }

  private getPath(operation: Operation): string {
    const path = operation.path
      .substring(1)
      .split('/')
      .map(node => {
        const num = Number(node)
        return isNaN(num) ? node : num
      })

    if (tuiIsNumber(path[path.length - 1])) {
      path.pop()
    }

    return path.join(' &rarr; ')
  }

  private getMessage(operation: Operation): string {
    switch (operation.op) {
      case 'add':
        return `${this.i18n.transform('added')} ${this.getNewValue(operation.value)}`
      case 'remove':
        return `${this.i18n.transform('removed')} ${this.getOldValue(operation.path)}`
      case 'replace':
        return `${this.i18n.transform('changed from')} ${this.getOldValue(
          operation.path,
        )} ${this.i18n.transform('to')} ${this.getNewValue(operation.value)}`
      default:
        return `Unknown operation` // unreachable
    }
  }

  private getOldValue(path: any): string {
    const val = getValueByPointer(this.originalValue, path)
    if (['string', 'number', 'boolean'].includes(typeof val)) {
      return val
    } else if (isObject(val)) {
      return this.i18n.transform('entry')!
    } else {
      return this.i18n.transform('list')!
    }
  }

  private getNewValue(val: any): string {
    if (['string', 'number', 'boolean'].includes(typeof val)) {
      return val
    } else if (isObject(val)) {
      return this.i18n.transform('new entry')!
    } else {
      return this.i18n.transform('new list')!
    }
  }
}
