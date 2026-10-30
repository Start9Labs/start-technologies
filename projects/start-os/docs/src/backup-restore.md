# Restoring Backups

Restore previously created backups to recover individual services or your entire server. This is for disaster recovery when a service is accidentally uninstalled or when your data drive is lost or corrupted.

## Backup Filesystem Formats

Existing format-1 backups remain available for backups and restores without conversion. Conversion is optional, but once started, it must finish before that server backup can be restored. Other server backups on the same target remain independent.

To resume an interrupted conversion, open **System → Restore Backup** on an initialized StartOS server, select the target's amber conversion control, and choose **Resume conversion** for the affected server backup. Enter the password used to encrypt that backup. Progress continues in the background; closing the dialog does not cancel it.

If initial setup cannot restore an interrupted backup, connect its target to an initialized StartOS server and resume conversion there before retrying setup recovery. The setup wizard does not automatically convert a backup. Converted backups cannot be read by older StartOS releases.

## Restoring Individual Services

This option should only be necessary if you accidentally uninstall a service.

1.  Go to `System -> Restore Backup`
1.  Select your backup drive.
1.  Decrypt the backup drive by entering the password that was used to create it.
1.  Select the service(s) you want to restore and click "Restore Selected".

> [!TIP]
> If the backup came from a different system architecture (x86, ARM, or RISC-V), StartOS runs its service images under emulation. After the restore, _reinstall_ or update each service from the marketplace so StartOS can select its package for the new server architecture. Do not uninstall it, since uninstalling deletes its data.

## Restoring an Entire Server

If your StartOS data drive is lost or corrupted and you need to restore your entire server, follow instructions [here](./initial-setup.md#recover-options).
