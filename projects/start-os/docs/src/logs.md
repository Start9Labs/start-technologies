# Logs

Every service, and StartOS itself, streams its logs live in the web interface. You can read them there, scroll back through older entries, and download a copy to keep or send to support. Nothing needs to be installed, and you do not need [SSH](ssh.md) or the command line.

## Service Logs

Open the service from the dashboard and select the **Logs** tab. New lines appear as the service writes them.

This is where a misbehaving service explains itself: a failed [health check](health-checks.md), a backend it cannot reach, a setting it rejected, or a crash on startup.

## OS and Kernel Logs

Go to **System**:

- **OS Logs** — StartOS itself: the daemon, service supervision, networking, and updates.
- **Kernel Logs** — the Linux kernel: hardware, disks, drivers, and out-of-memory events.

## Reading a Log

The view follows the end of the log, so new lines appear as they are written. Scroll up to read older entries — more are loaded as you reach the top. **Scroll to bottom** returns to the end and resumes following.

## Downloading Logs

**Download**, at the bottom of any log view, saves the most recent 10,000 entries as an HTML file that opens in any browser, colored the same as it appears onscreen. The file is named for what it came from — `<service-id>-logs.html`, `os-logs.html`, or `kernel-logs.html`.

Attach that file when you [contact support](https://start9.com/contact). It carries far more than the last error onscreen, and it is the fastest way to get an answer.

## From the Command Line

The web interface is the primary way to read logs. [`start-cli`](cli-reference.md) covers the cases it cannot reach — a server that will not boot far enough to serve the UI, or a log you want to pipe into another tool:

```bash
start-cli package logs <ID>
start-cli server logs
start-cli server kernel-logs
```

In **Diagnostic Mode**, where StartOS has failed to start normally, the diagnostic page offers **View logs**. There is no download button there, so `start-cli diagnostic logs` is how to capture those. See the [FAQ](faq.md#startos-boots-into-diagnostic-mode).
