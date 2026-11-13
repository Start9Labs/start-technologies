# Run a MySQL/MariaDB Sidecar

Some upstream services require MySQL or MariaDB instead of PostgreSQL. Each engine has its own health checks and lifecycle tools.

## Solution

Configure the database daemon with `--bind-address=127.0.0.1` and pass its database and password environment variables. Health-check MySQL by execing `mysql -e 'SELECT 1'`; MariaDB images can use their `healthcheck.sh` script. Back up the database by copying its volume with `sdk.Backups.ofVolumes()`; see [Databases: copy the volume or dump it](recipe-backups.md#databases-copy-the-volume-or-dump-it) for when `withMysqlDump()` or `withMariadbDump()` is the better choice. After a dump restore, a MariaDB daemon whose ready check is `healthcheck.sh` runs the official image's entrypoint with `MARIADB_AUTO_UPGRADE=1`, which adds the `healthcheck` users on its next start.

**Reference:** [Main](main.md) · [Initialization](init.md) · [Backups](recipe-backups.md)

## Examples

See `startos/main.ts` in: [ghost](https://github.com/Start9Labs/ghost-startos) (MySQL), [romm](https://github.com/Start9-Community/romm-startos) (MariaDB), and `startos/backups.ts` in [wordpress](https://github.com/Start9Labs/wordpress-startos) (MariaDB, copied volume)
