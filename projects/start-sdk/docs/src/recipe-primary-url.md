# Set a Primary URL

Some services need to know which URL they're hosted at — for generating links, sending invites, federating with other servers, or embedding in emails. Since StartOS services can be reached via multiple addresses (LAN, Tor, clearnet), the user must choose which URL the service treats as primary.

## Solution

Call `sdk.setupPrimaryUrl()` once, pointing it at the interface the URL belongs to and at the file-model field that holds the choice. It returns the "Set Primary URL" action to register, the init hook to add to `setupInit()`, and `read()`, a reactive reader for the choice. Read it in `setupMain()` and pass it to the service as an env var or config value.

Where the URL is an address of the service's own web UI, pass `read()` to `createInterface`'s `preferredLauncherAddress` in `setupInterfaces` as well, so StartOS's **Open UI** control opens the address the service is configured for instead of the one that suits the admin's connection. See [Choosing a Primary URL](interfaces.md#choosing-a-primary-url) for the code, and [Nominating an Address to Open](interfaces.md#nominating-an-address-to-open) for what a nomination does.

The hook stores the `.local` address when nothing is chosen yet, follows the chosen hostname through a port or scheme change, and when that hostname is no longer one of the interface's addresses stores `defaultUrl`'s pick in its place, the `.local` address unless the package says otherwise. It judges the choice against the addresses the interface has, not against what is reachable at that moment. A `.local` choice is judged only while some LAN interface is up, an IP choice only while the interface it came from is up, and a domain or Tor choice at once, so a link that is down leaves the choice standing. A service whose users must be told rather than moved, because callbacks or federation key off the URL, passes `onRemoved: 'task'`; that task is `important` by default, and `severity: 'critical'` is for a service that cannot run at all without a valid URL, because a critical task stops the service.

For a service whose hostname is permanent and cannot change after initial setup (Synapse), use a critical task on install with `visibility: 'hidden'` instead, so it's a one-time choice.

**Reference:** [Interfaces](interfaces.md#choosing-a-primary-url) · [Actions](actions.md) · [Initialization](init.md) · [Tasks](tasks.md)

## Examples

See `startos/` in: [synapse](https://github.com/Start9Labs/synapse-startos) (permanent server name)
