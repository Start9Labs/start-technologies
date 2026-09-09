import { enableProdMode } from '@angular/core'
import { bootstrapApplication } from '@angular/platform-browser'
import { AppComponent } from 'src/app/app.component'
import { APP_CONFIG } from 'src/app/app.config'
import { environment } from 'src/environments/environment'

if (environment.production) {
  enableProdMode()
}

bootstrapApplication(AppComponent, APP_CONFIG).catch(console.error)
