# Accessing Service Containers

Every service on StartOS runs inside its own isolated LXC container. Within each LXC container, the service runs in one or more **subcontainers**. StartOS provides `start-cli package attach` to open a shell inside a service's subcontainer — this is the supported way to access containers. StartOS does not use Docker or Podman; standard container tooling will not work. See the [CLI Reference](cli-reference.md) for the full list of `start-cli` options.

> [!WARNING]
> Accessing a service container is an advanced operation. Modifying files, stopping processes, or changing configuration inside the container can break the service or cause data loss. Proceed with caution.

## Why Access a Service Container?

There are several reasons you might want a shell inside a running service container:

- **Running CLI tools** — Many services ship their own command-line utilities (e.g., `bitcoin-cli`, `lncli`) that can be invoked directly inside the container.
- **Debugging** — Inspect running processes or file state when a service is misbehaving. Logs do not require a shell; they are on the service's [Logs](logs.md) tab in the UI.
- **Querying a database** — Run ad-hoc queries against a service's database (e.g., `sqlite3`, `psql`, `redis-cli`) that aren't exposed through the UI.
- **Inspecting configuration** — View the generated config files or environment variables a service is actually running with.
- **Advanced recovery** — In rare cases, manually repair data or state that cannot be fixed through the StartOS UI.

## Usage

First, [SSH](ssh.md) into your StartOS server. Then attach to a running service:

```bash
start-cli package attach <PACKAGE>
```

Replace `<PACKAGE>` with the package identifier (e.g., `bitcoind`, `lnd`). You can find package identifiers with:

```bash
start-cli package list
```

This drops you into a shell inside the service's **subcontainer** (not the LXC container itself). If the service has only one subcontainer, you are placed directly into it. If there is more than one subcontainer, you will be prompted to choose one. To skip the prompt, specify the subcontainer ID directly:

```bash
start-cli package attach <PACKAGE> <SUBCONTAINER>
```

Type `exit` or press `Ctrl+D` to return to the host.

## Accessing the LXC Container

In rare cases, you may need to access the LXC container itself rather than a subcontainer. For example, subcontainers are only accessible while the service is running, but the LXC container remains accessible even when the service is stopped — useful for inspecting or repairing state that prevents a service from starting.

First, obtain the container ID:

```bash
start-cli package stats <PACKAGE>
```

Then attach directly to the LXC container:

```bash
lxc-attach <CONTAINER-ID>
```

> [!WARNING]
> This bypasses StartOS's managed access layer. Only use this if you have a specific reason that `start-cli package attach` cannot fulfill.

## Service Data on the Host

A service's volumes live on the host at:

```
/media/startos/data/package-data/volumes/<PACKAGE>/data/<VOLUME>
```

Inside the container, the same volume is mounted at `/media/startos/volumes/<VOLUME>`. That shorter path is the container's view of it and does not exist on the host.

To see how much disk space each service is using, [SSH](ssh.md) in and run:

```bash
sudo du -h --max-depth=1 /media/startos/data/package-data/volumes/
```

> [!WARNING]
> Do not modify files under this path while the service is running.
