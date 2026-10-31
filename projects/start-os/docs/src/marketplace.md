# Marketplace

The Marketplace is where you discover services to install on your StartOS server. It is a collection of independent registries, like stores in a mall, each offering its own curated list of services.

## Registry System and Navigation

Each registry in the Marketplace is maintained independently. When you open the Marketplace, you browse one registry at a time. To switch between registries, open the registry selector above the search field. Its **Manage Registries** link opens **System → Manage Registries**, where you add or remove registries. Adding a registry asks you to review the keys it proposes for package trust before connecting. A registry that declares a description shows it above its services.

StartOS ships with two [default registries](default-registries.md): the **Start9 Registry** and the **Community Registry**. You can also add [alternative registries](alternative-registries.md) or [sideload](sideloading.md) packages directly.

## Package Signers

Package details show **Signed by** above **Version**, listing signing keys by GPG name and email addresses or saved label, with fingerprints for unknown keys. Disabled keys remain listed; trust permissions determine which signatures the server accepts. The install button is disabled when no trusted signer matches. Manage individual keys, their origins, and package and OS-update permissions under **System → [Manage Trust](trusted-keys.md)**.

## Open Ecosystem

Anyone, anywhere can establish their own registry. This open model means no single entity controls which services are available, and no service can be effectively censored. Users choose which signing keys to trust independently of the hosting registry, and services can be distributed through multiple independent registries, direct file transfer, or built from source.
