# Health Checks

Health checks monitor whether a service is ready and functioning. They appear on the dashboard alongside each running service. Health checks can be conditional — appearing or disappearing depending on what features are enabled or the current state of the service.

## Statuses

Each health check displays one of the following statuses:

- **Starting** — The health check is actively running but has not yet passed. Each health check has a grace period defined by the service developer. If the check does not pass within this period, it transitions to an error state.
- **Loading** — The health check is long-running and intentional. Rather than a pass/fail gate, it serves as a status monitor — a window into some ongoing activity in the service. For example, a Bitcoin node might display sync progress as a percentage.
- **Success** — The health check has passed. The service is ready and operational.
- **Error** — The health check has failed. The message will describe the problem.

Starting and loading checks show a clock when waiting for another condition and a spinner while actively progressing. Every status can display a message supplied by the service as-is. Empty or absent messages use the status label.

## What Gets Checked

Health checks are defined by each service and vary depending on what the service does. Common checks include whether a web interface is reachable, whether a database is accepting connections, or whether an API is responding.

A service may have multiple health checks. For example, a service with both a web UI and a background sync process might show separate status indicators for each.

## Internal Checks

Some health checks run behind the scenes and are not displayed in the UI. These monitor internal components, such as a database sidecar, that must be ready before the main service can start.
