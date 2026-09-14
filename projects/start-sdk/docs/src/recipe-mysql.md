# Run a MySQL/MariaDB Sidecar

Some upstream services require MySQL or MariaDB instead of PostgreSQL. Each engine has its own health checks, lifecycle tools, and dump-based backup builder.

## Solution

Configure the database daemon with `--bind-address=127.0.0.1` and pass its database and password environment variables. Health-check MySQL by execing `mysql -e 'SELECT 1'`; MariaDB images can use their `healthcheck.sh` script. Use `sdk.Backups.withMysqlDump()` for MySQL and `sdk.Backups.withMariadbDump()` for MariaDB. Each builder runs the engine's native initialization, server, client, and dump tools from the selected image. A MariaDB daemon whose ready check is `healthcheck.sh` runs the official image's entrypoint with `MARIADB_AUTO_UPGRADE=1`, which adds the `healthcheck` users to a restored data directory on its next start.

**Reference:** [Main](main.md) · [Initialization](init.md)

## Examples

See `startos/main.ts` in: [ghost](https://github.com/Start9Labs/ghost-startos) (MySQL), [romm](https://github.com/Start9-Community/romm-startos) (MariaDB)
