# Updating StartOS

> [!NOTE]
> There are _no_ automatic updates in StartOS. Updating the operating system and service always requires explicit action. That said, we highly recommend keeping StartOS up to date for the latest security and performance patches, as well as to take advantage of new features.

## Update through the UI

1. When a new version of StartOS is available, a rocket badge will appear on the "System" tab.

1. Go to `System -> General -> Software Update`.

1. Read the release notes and click "Begin Update". The UI blocks the update if the registry lists no signer trusted by your server for its update image.

   > [!WARNING]
   > Ensure you have a stable Internet connection before beginning an OS update, and do not unplug your server while StartOS is downloading.

1. While the new version of StartOS is downloading, you may continue to use your device as usual.

1. Once the download is complete, you will be prompted to restart your server to complete the update.

   > [!WARNING]
   > Updates can take up to an _hour_ to complete. During this time, there is no indication of progress and your StartOS UI will be unreachable. **DO NOT UNPLUG YOUR SERVER DURING THIS TIME!**

1. When your server comes back, a notification welcomes you to the new version. Open it from **Notifications** — the bell in the menu — to read that release's notes again at any time.

A browser page left open during the update shows **Refresh Needed** while its user interface syncs with the updated server. You can dismiss the dialog and finish your work while syncing continues. When syncing is complete, select **Refresh** to load the synced interface. A sync error offers **Retry**, **Details**, and **Reset UI cache**. If syncing stalls, **Reset UI cache** remains available; confirming it resets the cached interface and reloads the page while preserving your login and settings.

## Trusted update signers

Trusted OS update signers are stored on your server as public keys with labels. New installations and the migration initially trust drbonez, matthill, and Start9. Changes persist across restarts; removing a key, including all keys, does not restore the defaults.

Manage them using the CLI:

```sh
start-cli server update-signers list
start-cli server update-signers add "$(cat signer.pem)" --label "My release signer"
start-cli server update-signers get "$(cat signer.pem)"
start-cli server update-signers update "$(cat signer.pem)" --label "Renamed signer"
start-cli server update-signers remove "$(cat signer.pem)"
```

Keys accept PEM or DER64 format and identify entries; `list` returns DER64, and `update` changes the label. To replace a key, add its replacement and remove the old key. Labels do not grant trust to a different key. The UI uses this list before starting an update, and the downloader requires a valid signature from the accepted keys. Normal CLI updates and browser-console calls that omit signer requirements still verify signatures but do not enforce the saved trust list.

Diagnostic updates always require a valid signature from one of the built-in drbonez, matthill, or Start9 keys. Diagnostic mode cannot access the saved trust list.

## Update by Re-flashing

If you are updating to an unreleased version of StartOS, or something went wrong with a UI update (very rare), it may be necessary to update StartOS by re-flashing. Follow the guide for [Installing StartOS](installing-startos.md).
