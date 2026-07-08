# Package a Prebuilt Docker Image

The most common packaging task is wrapping an existing upstream Docker image — `linuxserver/*`, an official `org/app` image, a community image — rather than building your own from a `Dockerfile`. It looks simple, and the happy path is. But the same handful of mistakes sink these packages over and over: the image name is guessed instead of verified, only one of the image's data paths gets mounted, non-UI ports are forgotten, an image with its own init system crashes because it isn't PID 1, and credentials are "set" by hand-editing a config format that was never confirmed. This recipe is the checklist that keeps those from happening.

This page assumes the service shape from [Set Up a Basic Service](recipe-basic-service.md) — daemon, interface, health check, backup — and covers only what's different when you don't control the image. If you're starting a brand-new package, scaffold first (`start-cli s9pk init-package "My Service"`) and work its `TODO.md` top to bottom; this recipe expands the "replace the hello-world image" line of that checklist.

## Solution

Build the basic-service skeleton first, then apply these prebuilt-image concerns:

1. **Verify the image before you pin it.** Do not guess a `org/name`. Confirm the exact repository exists, the tag you want is published, and it ships the architectures StartOS needs (`x86_64` and `aarch64` at minimum). Pin `images.<id>.source.dockerTag` to that confirmed `image:tag` and set `arch` accordingly. See [Verify the image](#verify-the-image) below.
2. **Mount _every_ path the image persists.** Inspect the image (or its docs) for all data and config paths — there is usually more than one (e.g. a config dir _and_ a downloads/data dir). Mount each one, or that data lands on the container's ephemeral filesystem and is lost on every restart. See [Mount all data paths](#mount-all-data-paths).
3. **Expose every port the service needs — not just the web UI.** A torrent client needs its peer port; a mail server needs SMTP/IMAP; a database needs its wire port. Bind the UI in [`setupInterfaces()`](interfaces.md) and add the others via [Expose Multiple Interfaces](recipe-multi-interface.md). A constant like `peerPort` that is declared but never bound is a tell that a port was forgotten.
4. **Run the image's entrypoint, and make it PID 1 if it has its own init system.** Use `sdk.useEntrypoint()` to keep the upstream startup behavior. If the image bundles an init/supervisor — `s6-overlay` (every `linuxserver/*` image), `tini`, `dumb-init`, `supervisord` — set `runAsInit: true` on the daemon's `exec`, or the supervisor crashes because it is not PID 1. See [Images with their own init system](#images-with-their-own-init-system).
5. **Pass the env vars the image expects.** Many community images are configured through environment variables — `linuxserver/*` images read `PUID`, `PGID`, and `TZ` to drop privileges and set ownership; others take `APP_*` settings. Set them via `exec.env`. See [Pass Config via Environment Variables](recipe-env-vars.md).
6. **Apply credentials through the app's own mechanism — never a hand-written hash.** If the service needs an admin password, follow [Prompt User to Create Admin Credentials](recipe-admin-credentials.md). Do **not** invent the on-disk credential format; see [Credentials](#credentials).
7. **Verify by installing, not by compiling.** A clean `tsc` and a successful `s9pk pack` prove the code type-checks — not that the service runs. Install on a StartOS box, open the UI, and exercise the actual feature (log in, add data) before calling it done. See [Development Workflow — Verify against reality](workflow.md#verify-against-reality-not-against-tsc).

**Reference:** [Set Up a Basic Service](recipe-basic-service.md) (the underlying skeleton) · [Main](main.md) · [Manifest](manifest.md) · [Interfaces](interfaces.md)

## Verify the image

Before writing `dockerTag`, confirm three things from the registry — never from memory:

- **The repository exists** at the name you think it does. Image names are easy to misremember (`qbittorrentserver/qbittorrent` does not exist; `linuxserver/qbittorrent` does). Pulling, or fetching the registry's tags endpoint, tells you for sure.
- **The tag is published.** `latest` almost always exists; a specific `X.Y.Z` may not, or may be spelled differently (`5.2.1`, `v5.2.1`, `version-5.2.1`).
- **It is multi-arch.** Inspect the manifest list for `amd64`/`x86_64` and `arm64`/`aarch64`. An image that only ships `amd64` cannot target StartOS's ARM hardware.

```bash
# List published tags (Docker Hub library/community image):
curl -s "https://hub.docker.com/v2/repositories/linuxserver/qbittorrent/tags?page_size=25" \
  | jq -r '.results[].name'

# Confirm the tag is multi-arch:
docker manifest inspect linuxserver/qbittorrent:5.2.1 \
  | jq -r '.manifests[].platform.architecture'
```

```typescript
images: {
  qbittorrent: {
    source: { dockerTag: 'linuxserver/qbittorrent:5.2.1' },
    arch: ['x86_64', 'aarch64'],
  },
},
```

## Mount all data paths

Enumerate the paths the image writes to and persists — its documentation lists them, or you can run the image and watch where it creates files. Mount each path that must survive a restart. Missing a data mount does not produce an error; it silently discards that data on every restart, which is far worse.

```typescript
subcontainer: sdk.SubContainer.of(
  effects,
  { imageId: 'qbittorrent' },
  sdk.Mounts.of()
    .mountVolume({ volumeId: 'main', subpath: 'config',    mountpoint: '/config',    readonly: false })
    .mountVolume({ volumeId: 'main', subpath: 'downloads', mountpoint: '/downloads', readonly: false }),
  'qbittorrent-sub',
),
```

> A torrent client that mounts `/config` but not `/downloads` "works" in every quick test and loses every download the moment the service restarts. Map the data path, not just the config path.

## Images with their own init system

`linuxserver/*` images (and anything built on `s6-overlay`, `tini`, `dumb-init`, or `supervisord`) expect their init system to run as **PID 1**. In a StartOS subcontainer the daemon command is not PID 1 by default, so the supervisor aborts (s6 logs `s6-overlay-suexec: fatal: can only run as pid 1`). Set `runAsInit: true`:

```typescript
exec: {
  command: sdk.useEntrypoint(),
  runAsInit: true, // image bundles s6-overlay, which must be PID 1
  env: { PUID: '1000', PGID: '1000', TZ: 'Etc/UTC' },
},
```

See [Main — `runAsInit`](main.md#running-the-entrypoint-as-pid-1-runasinit) for the full description. If an image's bundled init system genuinely cannot be made to work, the fallback is to build your own image from a `Dockerfile` and invoke the binary directly — but reach for `runAsInit` first; it resolves the common case.

## Credentials

If the service has a web login, follow [Prompt User to Create Admin Credentials](recipe-admin-credentials.md): a `setupOnInit` watcher surfaces a critical task, and a `setAdminPassword` action generates, stores, and returns the credential.

The trap specific to prebuilt images is **how** the password reaches the application. Do not assume the on-disk format. Many apps store the web password as a salted **PBKDF2** or **bcrypt** value with app-specific framing — not as a bare hash you can compute and drop into a config key. Writing the wrong format does not error; the app silently rejects the login. So:

- Apply the credential through the app's **own** API or CLI (run it in `sdk.SubContainer.withTemp()` from the action — see [Reset a Password](recipe-reset-password.md)), **or**
- If you must write the config directly, first **confirm the real format** by setting a password through the app once and reading back exactly what it wrote.

Either way, **verify a real login succeeds** before shipping. A credential flow that has never been logged into is not done.

Two more traps surface only when you actually test the login:

- **Reverse-proxy guards.** StartOS fronts the service with its own proxy, so the request the app sees has a different `Host`/`Origin`/port than it served. Apps with host-header or CSRF validation (qBittorrent's `WebUI\HostHeaderValidation`, many others) reject every proxied request — often with a `401` that looks like a bad password but isn't. Check the app's log for the real reason, and disable the guard the app provides for running behind a proxy. Watch the inverse too: a "trust localhost" auth bypass can let proxy-local requests skip the password entirely — disable it.
- **Config you write while the app runs can be clobbered.** Many apps rewrite their whole config file on shutdown from in-memory state. If your action edits the config and then restarts the service, the shutdown flush overwrites your edit before the new instance reads it. Write config-file changes from `setupMain` _before_ the daemon launches (the previous instance has already stopped and flushed), or apply them through the running app's API instead.

## Examples

See `startos/main.ts` and `startos/manifest/index.ts` in packages that wrap prebuilt images: [ollama](https://github.com/Start9Labs/ollama-startos), [jellyfin](https://github.com/Start9Labs/jellyfin-startos), [vaultwarden](https://github.com/Start9Labs/vaultwarden-startos), [immich](https://github.com/Start9Labs/immich-startos), [home-assistant](https://github.com/Start9Labs/home-assistant-startos).

## Checklist

- [ ] Image repository, tag, and arches confirmed from the registry (not from memory)
- [ ] Every persisted path mounted (config **and** data)
- [ ] Every required port exposed (UI **and** non-UI)
- [ ] `sdk.useEntrypoint()` used; `runAsInit: true` if the image has its own init system
- [ ] Required env vars set (`PUID`/`PGID`/`TZ` for `linuxserver/*`, etc.)
- [ ] Credentials applied via the app's own mechanism; a real login verified
- [ ] Installed on a StartOS box and the feature exercised — not just `tsc` green
