# Set a Primary URL

Some services need to know which URL they're hosted at — for generating links, sending invites, federating with other servers, or embedding in emails. Since StartOS services can be reached via multiple addresses (LAN, Tor, clearnet), the user must choose which URL the service treats as primary.

## Solution

Create a "Set Primary URL" action using `sdk.Action.withInput()` with `Value.dynamicSelect()` that queries the service's own interfaces for available hostnames. The action persists the choice to a file model. In `setupMain()`, read the selected URL and pass it to the service as an env var or config value.

Where the URL the user picked is an address of the service's own web UI, pass it to `createInterface`'s `preferredLauncherAddress` in `setupInterfaces` as well, so StartOS's **Open UI** control opens the address the service is configured for instead of the one that suits the admin's connection. It has to be a whole URL, and it only does anything on the interface carrying that control — a service whose canonical URL is a bare hostname, or belongs to an interface other than its web UI, has nothing to nominate. See [Nominating an Address to Open](interfaces.md#nominating-an-address-to-open).

There are two variants. For services where the URL can change anytime (Ghost, Gitea, Vaultwarden), register a reactive watcher in `setupOnInit` that monitors the URL via `.const(effects)`. If the selected URL becomes unavailable (e.g., user disables a gateway), create a critical task prompting the user to pick a new one. For services where the hostname is permanent and cannot change after initial setup (Synapse), use a critical task on install with `visibility: 'hidden'` so it's a one-time choice.

**Reference:** [Actions](actions.md) · [Interfaces](interfaces.md) · [Initialization](init.md) · [Tasks](tasks.md)

## Examples

See `startos/actions/` and `startos/init/` in: [ghost](https://github.com/Start9Labs/ghost-startos) (changeable URL), [gitea](https://github.com/Start9Labs/gitea-startos) (changeable URL), [vaultwarden](https://github.com/Start9Labs/vaultwarden-startos) (changeable domain), [synapse](https://github.com/Start9Labs/synapse-startos) (permanent server name)
