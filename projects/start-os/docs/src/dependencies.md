# Dependencies

Dependencies are other services that a service requires or can optionally integrate with. They are listed on the service details page.

## Required vs Optional

- **Required** — The service declares this dependency in its package. Depending on what it needs, the dependency may need to be installed or running. For example, a Lightning node requires a Bitcoin node.
- **Optional** — The service works without this dependency but gains additional features when it is installed. For example, installing Tor service means Bitcoin can connect to peers over Tor.

## What Happens Automatically

StartOS lists required dependencies from the package as soon as it initializes the service. Optional integrations appear when the service activates them. Their published version and health requirements still apply when active. When you install a service, StartOS checks its dependencies. If a required dependency is not installed, you will be prompted to install it. StartOS also monitors version compatibility — if a dependency is installed but running an incompatible version, you will be notified.

## Configuring Dependencies

Some dependencies need to be configured to work with the services that depend on them. When this is the case, a [task](tasks.md) will appear on the dashboard guiding you through the necessary steps. These tasks often link directly to an [action](actions.md) on the dependency, with form fields pre-filled to simplify configuration.

## Service Communication

Once dependencies are installed and configured, services communicate with each other automatically over the local network. No manual networking setup is required.
