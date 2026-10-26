import { ErrorHandler, inject, Service } from '@angular/core'
import { TuiNotificationService } from '@taiga-ui/core'

import { HttpError } from '../classes/http-error'

@Service()
export class ErrorService extends ErrorHandler {
  private readonly alerts = inject(TuiNotificationService)

  override handleError(error: unknown, link?: string) {
    console.error(error)

    this.alerts
      .open(getErrorMessage(error, link), {
        label: 'Error',
        appearance: 'negative',
      })
      .subscribe()
  }
}

export function getErrorMessage(e: unknown, link?: string): string {
  let message = ''

  if (typeof e === 'string') {
    message = e
  } else if (e instanceof HttpError && e.code === 0) {
    message =
      "Request Error. Your browser couldn't reach your server (network or browser blocked the connection). Common causes: a VPN or firewall blocking loopback/LAN; the server is still starting; or the page is loaded from a different origin than the API. Try refreshing this page; if you're using a VPN, allow LAN connections or disconnect briefly."
  } else if (!(e instanceof Error || e instanceof HttpError) || !e.message) {
    message = 'Unknown Error'
  } else {
    message = e.message
  }

  return link
    ? `${message}<br /><br /><a href=${link} target="_blank" rel="noreferrer">Get Help</a>`
    : message
}
