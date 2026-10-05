# Flavors

Flavors are different services that share the same package ID. They are different implementations of the same protocol and data schema — for example, Bitcoin Core and Bitcoin Knots. Because they share the same data schema, flavors tend to satisfy the same dependent services, such as a lightning node or an Electrum server.

## Version Format

Flavor versions are designated by their exver prefix: `#flavor:upstream-semver:downstream-semver`. For more details on version formatting, see [Versions](/packaging/versions.html).

## Limitations

It is not supported to install multiple flavors of the same service simultaneously. However, you can switch between them. Running two side by side means maintaining your own fork of one under a different package ID — see [the FAQ](faq.md#can-i-run-two-copies-of-the-same-service).

## Switching Flavors

To switch from one flavor to another:

1. Go to the [Marketplace](marketplace.md).
1. Find and view the listing for the flavor you want to switch to.
1. Instead of an **Install**, **Update**, or **Downgrade** button, you will see a **Switch** button. Click it to switch.
