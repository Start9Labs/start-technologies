# Restoring Backups

Restore previously created backups to recover individual services or your entire server. This is for disaster recovery when a service is accidentally uninstalled or when your data drive is lost or corrupted.

## Restoring Individual Services

This option should only be necessary if you accidentally uninstall a service.

1.  Go to `System -> Restore from Backup`
1.  Select your backup drive.
1.  Decrypt the backup drive by entering the password that was used to create it.
1.  Select the service(s) you want to restore and click "Restore Selected".

> [!TIP]
> If the backup came from a different system architecture (x86, ARM, or RISC-V), StartOS runs its service images under emulation. After the restore, _reinstall_ or update each service from the marketplace so StartOS can select its package for the new server architecture. Do not uninstall it, since uninstalling deletes its data.

## Restoring an Entire Server

If your StartOS data drive is lost or corrupted and you need to restore your entire server, follow instructions [here](./initial-setup.md#recover-options).
