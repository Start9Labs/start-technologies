# Run a MySQL/MariaDB Sidecar

Some upstream services require MySQL or MariaDB instead of PostgreSQL. The pattern is similar but uses MySQL-specific health checks and backup tooling.

## Solution

Similar to PostgreSQL but with MySQL-specific health checks and backup. Configure the MySQL daemon with `--bind-address=127.0.0.1` and pass `MYSQL_ROOT_PASSWORD`, `MYSQL_DATABASE` as env vars. Health-check by execing `mysql -e 'SELECT 1'` or the MariaDB `healthcheck.sh` script. For backups, use `sdk.Backups.withMysqlDump()` with `engine: 'mysql'` or `engine: 'mariadb'`. Either engine runs from any image with the engine installed the way its distribution packages it — the official images, or a server from a distribution's packages inside another image. A daemon whose ready check is `healthcheck.sh` runs the official image's entrypoint with `MARIADB_AUTO_UPGRADE=1`, which adds the `healthcheck` users to a restored data directory on its next start.

**Reference:** [Main](main.md) · [Initialization](init.md)

## Examples

See `startos/main.ts` and `startos/backups.ts` in: [ghost](https://github.com/Start9Labs/ghost-startos) (MySQL), [romm](https://github.com/Start9-Community/romm-startos) (MariaDB)
