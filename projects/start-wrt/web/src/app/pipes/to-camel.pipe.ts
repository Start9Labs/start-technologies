import { Pipe, PipeTransform } from '@angular/core'

@Pipe({
  name: 'toCamel',
})
export class ToCamelPipe implements PipeTransform {
  public transform(value: string): string {
    return value.toLowerCase().replaceAll(' ', '-')
  }
}
