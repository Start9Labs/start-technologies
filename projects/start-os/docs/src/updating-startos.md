# Updating StartOS

> [!NOTE]
> There are _no_ automatic updates in StartOS. Updating the operating system and service always requires explicit action. That said, we highly recommended keeping StartOS up to date for the latest security and performance patches, as well as to take advantage of new features.

## Update through the UI

1. When a new version of StartOS is available, a rocket badge will appear on the "System" tab.

1. Go to `System -> General -> Software Update`.

1. Read the release notes and click "Begin Update".

   > [!WARNING]
   > Ensure you have a stable Internet connection before beginning an OS update, and do not unplug your server while StartOS is downloading.

1. While the new version of StartOS is downloading, you may continue to use your device as usual.

1. Once the download is complete, you will be prompted to restart your server to complete the update.

   > [!WARNING]
   > Updates can take up to an _hour_ to complete. During this time, there is no indication of progress and your StartOS UI will be unreachable. **DO NOT UNPLUG YOUR SERVER DURING THIS TIME!**

1. When your server comes back, a notification welcomes you to the new version. Open it from the notifications bell, or from `System -> Notifications`, to read that release's notes again at any time.

## Update by Re-flashing

If you are updating to an unreleased version of StartOS, or something went wrong with a UI update (very rare), it may be necessary to update StartOS by re-flashing. Follow the guide for [Installing StartOS](installing-startos.md).
