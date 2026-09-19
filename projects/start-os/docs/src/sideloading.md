# Sideloading

Sideloading lets you install a service from a `.s9pk` file without using any registry. This is useful when testing a service in development, installing a service that is not listed on any registry, running a [second copy of a service](faq.md#can-i-run-two-copies-of-the-same-service), or if you prefer to eliminate the Marketplace as a point of trust.

## How to Sideload

1. Click **Sideload** in the top navigation bar.

1. Click the drop area to select a `.s9pk` file from your file system, or simply drag and drop the file into the drop area.

If installation fails, StartOS displays the service's error in the sideload progress view. Very long messages are shortened safely to fit the progress stream.

## Obtaining `.s9pk` Files

A `.s9pk` file can be obtained in several ways:

- **From a developer** — A service developer may share `.s9pk` files directly via chat, email, or a download link.
- **From a GitHub release** — Many open source services publish `.s9pk` files as release assets on GitHub.
- **Build from source** — If the service is open source, you can clone its repository and build the `.s9pk` yourself. See the [Makefile Build System](/packaging/makefile.html) guide for instructions.
