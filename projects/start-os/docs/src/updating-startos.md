# Updating StartOS

StartOS notifies you when your selected OS registry announces a compatible security update for your hardware. These warnings also use [browser push](notifications.md) when enabled. Use the notification's **Software Update** link to open `System -> General`, then select **Software Update** to review it. Installation requires your approval.

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

## Trusted Update Signers

Open **System → Trusted keyrings** and choose **Edit** from a row's overflow menu to select publishers for OS updates. Package and OS-update trust switches are independent. See [Trusted Keyrings](trusted-keys.md) for defaults, adding keys, migration behavior, and CLI management.

CLI OS updates enforce the saved OS-update trust policy unless explicitly overridden with `--trust`. Signature and integrity verification remain mandatory.

## Update by Re-flashing

If you are updating to an unreleased version of StartOS, or something went wrong with a UI update (very rare), it may be necessary to update StartOS by re-flashing. Follow the guide for [Installing StartOS](installing-startos.md).
