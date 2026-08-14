# Run a Nested OCI Runtime

Some services run their own containers — CI runners (`gitea-act-runner`, Forgejo Runner, Drone) execute every job inside a fresh OCI container; build daemons (`buildkitd`) launch sandboxes per build; emulator services pull and run arbitrary images on demand. Without a real container engine inside the service, those workloads can't be sandboxed properly and the service can't isolate untrusted user code.

StartOS supports running a rootless OCI engine — Podman or Docker — inside an opt-in service. A nested engine needs two manifest flags: `userspaceFilesystems: true` exposes `/dev/fuse` for `fuse-overlayfs` storage, and `virtualNetworking: true` exposes `/dev/net/tun` for `slirp4netns`/`pasta` networking. The service's own LXC remains userns-mapped and AppArmor-confined; nothing about the host's posture changes.

## Solution

1. Set both `userspaceFilesystems: true` and `virtualNetworking: true` at the manifest top level — fuse for storage, tun for rootless networking.
2. Bake the OCI engine and its rootless prerequisites into the service image.
3. Add a non-root user and `/etc/subuid` / `/etc/subgid` ranges that fit inside the subcontainer's user namespace.
4. (Docker only) Drop a tiny `runc` wrapper into the image and point `default-runtime` at it via `daemon.json` — Docker injects a `net.ipv4.ip_unprivileged_port_start` sysctl by default that runc fails to apply across the nested-userns boundary.

Podman works out of the box once the prerequisites are in place. Docker needs the wrapper.

**Reference:** [Manifest](./manifest.md) · [Project Structure](./project-structure.md)

## Manifest

```typescript
import { setupManifest } from '@start9labs/start-sdk'
import { short, long } from './i18n'

export const manifest = setupManifest({
  id: 'gitea-runner',
  title: 'Gitea Actions Runner',
  // ...
  volumes: ['main'],
  images: {
    main: {
      source: { dockerBuild: { workdir: '.' } },
      arch: ['x86_64', 'aarch64'],
    },
  },
  dependencies: {},
  userspaceFilesystems: true,
  virtualNetworking: true,
})
```

## What StartOS provides

With `userspaceFilesystems` and `virtualNetworking` set, the per-service LXC gets:

- `/dev/fuse` — char device 10:229, world-RW (via `userspaceFilesystems`). Required by `fuse-overlayfs` for rootless layered storage. Kernel overlayfs-on-overlayfs is denied for unprivileged users, so fuse-overlayfs is the only viable rootless storage driver inside a userns LXC.
- `/dev/net/tun` — char device 10:200, world-RW (via `virtualNetworking`). Required by `slirp4netns` and `pasta` for rootless container networking.

`virtualNetworking` additionally grants `CAP_NET_ADMIN` (scoped to the container's user namespace). A rootless OCI engine using `slirp4netns`/`pasta` doesn't strictly need it, but the tun device and the capability are bundled under the one flag; the grant is namespaced and harmless here.

Both devices are re-created inside the container with the host node's device numbers and permissions, then bind-mounted onto their usual `/dev` paths (via the same machinery that handles `hardwareAcceleration` for GPU nodes). The host's `fuse` and `tun` kernel modules are auto-loaded at boot.

The host-level sysctls `kernel.unprivileged_userns_clone=1` and `user.max_user_namespaces=28633` are pinned at install time so unprivileged user-namespace creation is allowed and headroom for nested namespaces is reserved.

## Image: Podman

```dockerfile
FROM debian:trixie-slim

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      podman fuse-overlayfs uidmap iproute2 iptables \
      slirp4netns ca-certificates \
 && apt-get clean \
 && rm -rf /var/lib/apt/lists/*

RUN mkdir -p /etc/containers && \
    printf 'unqualified-search-registries = ["docker.io"]\n' \
        > /etc/containers/registries.conf

# Subordinate UIDs/GIDs for nested user namespaces. The range MUST live
# inside the subcontainer's userns (mapped 0..65535) and MUST NOT overlap
# with the calling user's own UID — kernel rejects uid_map writes with
# EINVAL when outside ranges overlap.
RUN useradd --create-home --uid 1000 --shell /bin/bash app \
 && echo 'app:1001:64535' > /etc/subuid \
 && echo 'app:1001:64535' > /etc/subgid

USER app
WORKDIR /home/app
```

Inside `setupMain`, run Podman as `app`:

```bash
podman --root=$HOME/.local/share/containers/storage \
       --runroot=$XDG_RUNTIME_DIR/containers \
       --cgroup-manager=cgroupfs \
       run --network=slirp4netns --rm docker.io/library/alpine echo ok
```

(`--cgroup-manager=cgroupfs` is required because there's no user systemd session inside the subcontainer.)

## Image: Docker

Docker rootless needs the same prerequisites — subuid, fuse-overlayfs, slirp4netns — plus one workaround. Docker's container spec includes a `net.ipv4.ip_unprivileged_port_start=0` sysctl by default; runc opens that proc file in the parent userns and re-opens the file descriptor inside the nested userns, where it EPERMs. The kernel itself is fine with the write — `unshare -Urn` from the same shell can do it — but runc's reopen-after-pivot pattern breaks under nested userns. Setting `--sysctl net.ipv4.ip_unprivileged_port_start=…` on the command line doesn't help: runc still does the reopen.

The fix is a thin runc wrapper that strips that sysctl from the OCI bundle before exec'ing real runc. Drop it in the image, register it as the default runtime in `/etc/docker/daemon.json`. Skipping the sysctl is harmless — ports < 1024 just stay privileged inside the container, which is the upstream Linux default anyway.

```dockerfile
FROM debian:trixie-slim

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      curl gnupg fuse-overlayfs uidmap iproute2 iptables \
      slirp4netns jq ca-certificates \
 && install -m 0755 -d /etc/apt/keyrings \
 && curl -fsSL https://download.docker.com/linux/debian/gpg \
        -o /etc/apt/keyrings/docker.asc \
 && chmod a+r /etc/apt/keyrings/docker.asc \
 && echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/debian trixie stable" \
        > /etc/apt/sources.list.d/docker.list \
 && apt-get update \
 && apt-get install -y --no-install-recommends \
      docker-ce docker-ce-cli containerd.io docker-ce-rootless-extras \
 && apt-get clean \
 && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --uid 1000 --shell /bin/bash app \
 && echo 'app:1001:64535' > /etc/subuid \
 && echo 'app:1001:64535' > /etc/subgid

# runc wrapper — strips the sysctl runc can't apply across the nested
# userns boundary. See https://github.com/Start9Labs/start-technologies/pull/3209.
COPY runc-nested.sh /usr/local/bin/runc-nested
RUN chmod +x /usr/local/bin/runc-nested

# Tell dockerd to use it as the default runtime.
RUN mkdir -p /etc/docker
COPY daemon.json /etc/docker/daemon.json
```

`runc-nested.sh`:

```sh
#!/bin/sh
# Strip net.ipv4.ip_unprivileged_port_start from the OCI spec — runc's
# reopen of that sysctl across a nested userns boundary EPERMs in a
# StartOS service subcontainer. Skipping it is harmless; ports < 1024
# just stay privileged inside the container.
set -e
bundle=""
prev=""
for arg in "$@"; do
    case "$prev" in --bundle|-b) bundle="$arg"; break;; esac
    case "$arg" in --bundle=*) bundle="${arg#--bundle=}"; break;; esac
    prev="$arg"
done
cfg="${bundle}/config.json"
if [ -n "$bundle" ] && [ -f "$cfg" ]; then
    tmp=$(mktemp "${cfg}.XXXXXX")
    jq 'del(.linux.sysctl["net.ipv4.ip_unprivileged_port_start"])' \
       "$cfg" > "$tmp" && mv "$tmp" "$cfg"
fi
exec /usr/bin/runc "$@"
```

`daemon.json`:

```json
{
  "storage-driver": "fuse-overlayfs",
  "default-runtime": "runc-nested",
  "runtimes": {
    "runc-nested": { "path": "/usr/local/bin/runc-nested" }
  }
}
```

Once dockerd is running (rootful, since `dockerd-rootless.sh` requires the calling user to not be uid 0 and there's no user systemd session inside the subcontainer), default `docker run` and `docker build` work with bridge networking and no extra flags.

## Caveats

- **No daemon manager.** There's no systemd-user session inside the subcontainer, so engines that prefer systemd cgroups need `--cgroup-manager=cgroupfs` (Podman) or run rootful (Docker). Either is fine; just don't expect `loginctl enable-linger` or user-scoped systemd units.
- **Subordinate-UID range overlap.** `/etc/subuid` / `/etc/subgid` ranges must live inside the subcontainer's userns (UIDs 0..65535) AND must not overlap with the calling user's own UID. With `useradd --uid 1000`, the subordinate range must skip 1000 — `app:1001:64535` works, `app:1:65535` does not.
- **`fuse-overlayfs` only.** Kernel overlayfs-on-overlayfs is denied for unprivileged users, so don't try `--storage-driver=overlay2`. `fuse-overlayfs` is the only rootless option.
- **No bridge IPv6 by default.** Rootless networking via slirp4netns is IPv4-only out of the box. If you need IPv6 inside nested containers, configure pasta (`--network=pasta`) instead.
- **The capability flags are independent.** `userspaceFilesystems`, `virtualNetworking`, and `hardwareAcceleration` are orthogonal opt-ins. A nested OCI engine needs the first two; an LLM-driven CI runner that also wants GPU access sets all three.
