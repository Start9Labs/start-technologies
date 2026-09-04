# Verification

Start9 publishes a list of registries it has verified. A verified registry is offered in the **Add a Registry** picker on every StartOS server and in the [Start9 Marketplace](https://marketplace.start9.com), carries a **Verified by Start9** mark beside its name, and is shown under the name, icon, and description Start9 recorded for it rather than whatever its server reports at the moment. StartOS servers read the list from Start9, so a listing reaches users without a StartOS release.

## What verification means

Verification is a statement about the operator, not about the services:

- **The address is the one Start9 lists under that name.** A registry at any other address cannot present a verified registry's name; StartOS shows such a registry by its address instead and says why.
- **Start9 knows who operates the registry and how to reach them**, and the operator has agreed to the terms below.

It is not an endorsement. Start9 does not review, maintain, or support the services on a verified registry unless it operates the registry itself, and every listing carries a notice saying so, shown to users while the registry is selected.

## Requirements

To be verified, a registry must:

- Run the StartOS Registry service or `start-registry`, reachable over HTTPS at a stable address the operator controls.
- Serve packages built and signed with the StartOS SDK that follow this guide.
- Declare a name, icon, and description through **Configure Registry** that don't imitate Start9's registries or another verified registry.
- Have an operator who has given Start9 their real-world identity — a legal name or organization and a working contact — and who agrees to keep that contact current, respond to reports about a package within a reasonable time, and remove packages that turn out to be malicious or infringing.

Start9 may decline a listing or remove one at any time, and will remove one if the operator can't be reached or the registry no longer meets these requirements.

## How to apply

1. Ask in the [service packaging room](https://matrix.to/#/#dev-service-packaging:matrix.start9labs.com) on Matrix, with the registry's address and the operator's contact. Start9 will arrange to confirm your identity.
2. Open a pull request against [start-technologies](https://github.com/Start9Labs/start-technologies) adding your registry to `shared-libs/ts-modules/shared/well-known/startos/registries.json`: its `url`, the `name`, `icon` (a data URL), and `description` it serves, and the `warning` users will see. The warning must say that Start9 does not operate the registry or vouch for its services; the rest of the wording is yours. `description` and `warning` take a plain string or a map of locale to string.
3. Once your identity is confirmed and the pull request is merged, the listing is live on the next marketplace deploy.

## Keeping the listing current

The listed name, icon, and description are what users see, so change them by pull request against the same file, and change the registry to match at the same time. While a registry is listed, its own name and icon are not displayed, and its own description is displayed only when the listing has none.
