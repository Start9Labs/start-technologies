# Installing

There are several ways to install and manage services on StartOS.

## From the Marketplace

Open the [Marketplace](marketplace.md), select a service, and click **Install**. The Marketplace ships with the [Start9 Registry and the Community Registry](default-registries.md), and you can add [custom registries](alternative-registries.md).

## Sideloading

You can install services directly from `.s9pk` files without using a registry. See [Sideloading](sideloading.md).

## Updating

When newer versions of installed services are available, you can update them from the Updates tab or directly from the Marketplace. See [Updating](updating.md).

## Downgrading

If a service listing shows an older version than what you have installed, the Marketplace will display a **Downgrade** button instead of Install or Update.

Not every service can go back — the older version has to be able to take over the data the service has now, and StartOS refuses the downgrade when it cannot, leaving the running service untouched. To go back anyway, uninstall the service — which deletes its data — and [restore a backup](backup-restore.md) made while it was running that version.

## Switching Flavors

If multiple [flavors](flavors.md) of a service exist, the Marketplace will display a **Switch** button when viewing a different flavor than the one currently installed.
