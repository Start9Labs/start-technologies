# Updating

StartOS makes it easy to keep your services up to date. When updates are available, a badge appears on the **Updates** tab in the top navigation bar showing the number of pending updates.

The Updates tab lists, for each installed service, the newest version you can install now. A version that needs a newer StartOS, or that your installed version cannot migrate to directly, is not listed; when a service must update in steps, the tab offers the next step.

## Using the Updates Tab

1. Click the **Updates** tab in the top navigation bar.
1. Updates are organized by registry. Click a registry to list its available updates.
1. Click an update to expand it and see who signed it and the release notes of every version since the one you have installed, newest first. When several versions are included, expand a version to read its notes; the newest starts open. An update not signed by a key you trust shows a warning, and installing it asks you to confirm the risks first.
1. Click **Update**, in the details or in the update's row. If the package has a pre-download alert for your installed version, read it and select **Continue** to start the download or **Cancel** to leave the service unchanged.

## Switching Registries

It is possible to update a service from a different registry than the one it was originally installed from. If you do this, StartOS will warn you about the registry switch before proceeding.

## Updating from the Marketplace

You can also update a service directly from the [Marketplace](marketplace.md). Navigate to the service's page and click **Update**. The same pre-download alert appears there when it applies. The service's page opens on the version you can update to. You can still select a newer version, but its **Update** button is disabled with a note explaining why: your installed version cannot update to it directly, or it needs a newer version of StartOS, in which case update your server first.
