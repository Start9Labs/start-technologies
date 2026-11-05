# Actions

Actions are GUI representations of commands, API calls, or scripts that a system administrator would ordinarily run on the command line — complex, often dangerous operations that require time and expertise. In StartOS, service developers wrap these tasks into simple buttons with optional validated form input, making them accessible to anyone. They appear in the sidebar of the service details page.

## Examples

Actions vary by service. Common examples include:

- **Retrieve credentials** — Display an auto-generated admin username and password. Sensitive values are masked by default and can be copied or revealed.
- **Toggle a setting** — Enable or disable a feature, such as public registrations.
- **Configure an integration** — Fill out a form to connect the service with an external system, such as an SMTP server for sending email.
- **Reset a password** — Generate a new password and display it.

## Using Actions

Click an action in the sidebar to run it. Some actions execute immediately and display a result. Others present a form for you to fill out before submitting.

A form can ask you to select files. Uploads start after you submit, with a progress bar for each file; the action runs once every upload finishes. If an upload or the action fails, the form keeps your selections for another submission.

An action can also return a file to download. Click **Download** in its result. Download links are single-use and expire after 30 minutes; run the action again if you need another link.

## When Actions Are Available

Not all actions are available at all times. Some actions only appear — or can only be run — when a service is running, stopped, or in a particular state. If an action is unavailable, it will be grayed out or hidden.

## Warnings

Some actions display a warning before execution, alerting you to potential consequences. Read these carefully before proceeding.
