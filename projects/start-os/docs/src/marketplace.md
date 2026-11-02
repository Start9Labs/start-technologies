# Marketplace

The Marketplace is where you discover services to install on your StartOS server. It is a collection of independent registries, like stores in a mall, each offering its own curated list of services.

## Registry System and Navigation

Each registry in the Marketplace is maintained independently. When you open the Marketplace, you browse one registry at a time. To switch between registries, open the registry selector above the search field. Its gear opens **System → Registries**. A registry that declares a description shows it above its services.

StartOS ships with two [default registries](default-registries.md): the **Start9 Registry** and the **Community Registry**. You can also add [alternative registries](alternative-registries.md) or [sideload](sideloading.md) packages directly.

## Managing Registries

**System → Registries** lists your registries. The one that provides StartOS updates carries a **StartOS updates** badge. Click **Add registry** and enter a registry's URL to add it; the keys it offers that your server does not already have become trusted for packages, and StartOS lists them. A registry's menu offers:

- **Manage keys** opens [Trusted Keys](trusted-keys.md) filtered to the keys the registry offers.
- **Use for StartOS updates** makes the registry the source of StartOS updates and trusts its keys for StartOS updates. StartOS first checks that the registry publishes StartOS updates, and warns before using any registry other than the Start9 Registry.
- **Remove** removes the registry. Afterward, StartOS offers to remove the keys it added that no connected registry still offers. The Start9 Registry and the Community Registry cannot be removed, and neither can the registry that provides StartOS updates until you choose another.

## Package Signers

Package details show **Signed by** above **Version**, listing signing keys by label and email addresses, with fingerprints for unknown keys. Untrusted keys remain listed; trust selections determine which signatures the server accepts. When no trusted signer matches, a warning links to Trusted Keys, and installing asks you to confirm the risks first. Manage individual keys and their package and StartOS-update trust under **System → [Trusted Keys](trusted-keys.md)**.

## Open Ecosystem

Anyone, anywhere can establish their own registry. This open model means no single entity controls which services are available, and no service can be effectively censored. Users choose which signing keys to trust independently of the hosting registry, and services can be distributed through multiple independent registries, direct file transfer, or built from source.
