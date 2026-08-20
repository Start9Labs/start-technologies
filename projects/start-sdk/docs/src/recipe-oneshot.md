# Run a One-Shot Command

Before the main daemon starts, you may need to fix file ownership, run the app's own database migrations, or perform other idempotent setup. Oneshots run to completion and block dependent daemons until they finish.

A oneshot runs on **every** start, so it fits work whose answer can differ next time. Work keyed to the package version — relocating or repairing data an older release left behind — belongs in `migrations.up` instead, even when you can guard it to be idempotent. See [main.md § Choosing Between a Oneshot, an Init, and a Migration](main.md#choosing-between-a-oneshot-an-init-and-a-migration).

## Solution

Use `.addOneshot()` in the daemon chain. Oneshots run to completion and block dependent daemons via the `requires` array. Use `exec.command` for simple shell commands (e.g., `chown`) or `exec.fn` for complex async logic. Oneshots run on every service start, not just once — they must be idempotent. A post-startup oneshot can depend on a daemon (`requires: ['app']`) to run after the app is healthy.

**Reference:** [Main](main.md)

## Examples

See `startos/main.ts` in: [ghost](https://github.com/Start9Labs/ghost-startos) (chown-mysql), [immich](https://github.com/Start9Labs/immich-startos) (configure-libraries), [nextcloud](https://github.com/Start9Labs/nextcloud-startos) (chown), [btcpayserver](https://github.com/Start9Labs/btcpayserver-startos)
