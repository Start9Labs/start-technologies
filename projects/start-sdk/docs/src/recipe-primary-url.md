# Set a Primary URL

Some services need to know which URL they're hosted at — for generating links, sending invites, federating with other servers, or embedding in emails. Since StartOS services can be reached via multiple addresses (LAN, Tor, clearnet), the user must choose which URL the service treats as primary.

## Solution

Call `sdk.setupPrimaryUrl()` with the interface the URL belongs to and `get`/`set` functions over the file model that stores the choice. Register the `action` it returns, and in `setupMain()` pass `await primaryUrl.bestUsable(effects).const()` to the service as an env var or config value. That is the stored URL while its hostname is one of the interface's addresses, and the `.local` address otherwise, so the service keeps running while the chosen address is gone and returns to it when it comes back.

Where the URL is an address of the service's own web UI, pass the same read to `createInterface`'s `preferredLauncherAddress` in `setupInterfaces` as well, so StartOS's **Open UI** control opens the address the service is configured for instead of the one that suits the admin's connection. See [Choosing a Primary URL](interfaces.md#choosing-a-primary-url) for the code, and [Nominating an Address to Open](interfaces.md#nominating-an-address-to-open) for what a nomination does.

To tell the user when the choice is unset or gone, list `primaryUrl.setupTask(severity, { reason })` after `actions` in `setupInit()`. StartOS clears the task once the stored URL is one of the interface's addresses again.

For a service whose hostname is permanent and cannot change after initial setup (Synapse), use a critical task on install with `visibility: 'hidden'` instead, so it's a one-time choice.

**Reference:** [Interfaces](interfaces.md#choosing-a-primary-url) · [Actions](actions.md) · [Initialization](init.md) · [Tasks](tasks.md)

## Examples

See `startos/` in: [synapse](https://github.com/Start9Labs/synapse-startos) (permanent server name)
