# Tasks

Tasks are notifications that prompt you to take action on a service. They appear on the dashboard and guide you through setup, configuration, and ongoing maintenance.

## Task Severity Levels

Tasks have three levels:

- **Required** — The service cannot be started while a required task exists. If a required task is created while the service is running, the service will be forcibly stopped. This means the service cannot safely run until the task is completed. For example, retrieving an auto-generated admin password after installation.
- **Important** — The service will still run, but there might be issues. For example, configuring email settings for a service that uses notifications.
- **Recommended** — The service will run fine without this, but it is something you should consider doing. For example, an optional integration that improves functionality.

## Common Task Types

### Setup Tasks

After installing a service, you will often see tasks guiding you through initial setup — such as retrieving credentials, configuring integrations, or running a required action.

### Dependency Tasks

Some tasks prompt you to configure a _different_ service so it can work with the one you just installed. For example, installing a chat service might create a task asking you to register it with your Matrix homeserver. These tasks link directly to the relevant action on the dependency, sometimes with form fields pre-filled.

## Completing Tasks

Click a task to view its details. Most tasks direct you to run a specific [action](actions.md) on either the service itself or one of its dependencies. Once the required action is completed, the task clears automatically.

Some tasks will reappear if the underlying condition is no longer met — for example, if a required configuration is changed or removed.
