import { HttpErrorResponse } from '@angular/common/http'

export class HttpError {
  constructor(private readonly error: HttpErrorResponse) {}

  readonly code = this.error.status
  readonly message =
    typeof this.error.error === 'string' &&
    this.error.error &&
    this.error.headers
      .get('content-type')
      ?.split(';')[0]
      ?.trim()
      .toLowerCase() === 'text/plain'
      ? this.error.error
      : this.error.message
}
