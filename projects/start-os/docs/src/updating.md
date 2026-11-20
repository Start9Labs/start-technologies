# Updating

StartOS makes it easy to keep your services up to date. When updates are available, a badge appears on the **Updates** tab in the top navigation bar showing the number of pending updates. The badge turns red when any of them is a security update.

StartOS notifies you when a security update arrives for a service you have installed. Until you read that notification, later security updates are added to it rather than sent as new ones. These notifications also use [browser push](notifications.md) when enabled. Use the notification's **Updates** link to open the Updates tab.

## Using the Updates Tab

1. Click the **Updates** tab in the top navigation bar.
1. Updates are organized by registry. Click a registry to list its available updates. A registry offering a security update shows a red badge, and a security update shows its version in red with a shield icon.
1. Click an update to expand it and see who signed it and the release notes of every version since the one you have installed, newest first. Notes from a version that fixes a security vulnerability begin with a **Security update** label. **View in Marketplace** opens its full listing. An update not signed by a key you trust shows a warning, and installing it asks you to confirm the risks first.
1. Click **Update**, in the details or in the update's row. If the package has a pre-download alert for your installed version, read it and select **Continue** to start the download or **Cancel** to leave the service unchanged.

## Switching Registries

It is possible to update a service from a different registry than the one it was originally installed from. If you do this, StartOS will warn you about the registry switch before proceeding.

## Updating from the Marketplace

You can also update a service directly from the [Marketplace](marketplace.md). Navigate to the service's page and click **Update**. The same pre-download alert appears there when it applies.
