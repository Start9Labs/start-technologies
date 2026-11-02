# Uninstalling

Uninstalling removes a service from StartOS. A regular uninstall deletes the service's data with it, and a soft uninstall keeps the data for when you install the service again.

## Uninstalling a Service

1. Open the service and select **Actions**.
1. Under **StartOS**, select **Uninstall**.
1. StartOS warns that uninstalling the service will permanently delete its data. Select **Uninstall** to go ahead.

If other installed services depend on it, the warning also says that they will no longer work properly and may crash. See [Dependencies](dependencies.md).

Uninstalling deletes the service's data and its logs. Your backups are untouched, so you can bring the service back by [restoring it from a backup](backup-restore.md).

## When a Service Fails to Launch

When a service fails to launch, its page shows a **Service Launch Error** card, with the error and three buttons:

- **Rebuild container** — Rebuilds the service's container. It is harmless, takes a few seconds, and will likely resolve the error. Try it first.
- **Soft uninstall** — Removes the service from StartOS, and keeps its data and logs.
- **Hard uninstall** — Removes the service from StartOS, and deletes its data and logs. StartOS asks you to confirm first.

Both uninstall buttons force the removal: StartOS removes the service even when the service's own uninstall step fails or can't run. If other installed services depend on it, StartOS warns you before either one.

## Reinstalling After a Soft Uninstall

Install the same service again, from the [Marketplace](marketplace.md) or by [sideloading](sideloading.md). StartOS finds the data the soft uninstall kept, and installs the service as an update from the version that wrote that data.

## From the Command Line

`start-cli package uninstall <ID>` uninstalls a service and deletes its data. `--soft` keeps the data, and `--force` removes the service even when its own uninstall step fails or can't run. See the [CLI reference](cli-reference.md#start-cli-package-uninstall-id).
