# Passkeys

A passkey signs you in to StartOS without the master password. Its private key stays in your phone, computer, security key or password manager, and StartOS stores only the public half.

## Adding a Passkey

Go to **System > Passkeys**, click **Add**, give the passkey a name and enter your master password. Your browser then asks you to create the passkey with your device's lock, a security key or your password manager. The passkey must be stored on that device or password manager (a discoverable credential); security keys that cannot store one are not supported.

A passkey works only at the address it was created at. A passkey added while you were on your server's `.local` address does not work on its `.onion` address or a custom domain; add one at each address you sign in from. Browsers do not allow passkeys at a raw IP address.

## Signing In With a Passkey

On the login page, click **Login with passkey** and confirm with your device. StartOS asks again each time you open the UI in a new browser tab or reload it, and after the UI has been disconnected for ten minutes, for example while your computer sleeps. Nothing from a passkey sign-in is saved in the browser, and a passkey sign-in ends ten minutes after its tab closes.

## Removing a Passkey

Go to **System > Passkeys** and click the trash icon next to the passkey. Removing it signs out every session that passkey started. Passkeys stay registered when you log out, and removing them here is how you revoke one, for example after losing the device that holds it.

## Passkeys as Signing Keys

The public key of a passkey can also be added as a [trusted key](trusted-keys.md), so that packages and StartOS updates signed with that passkey are accepted.
