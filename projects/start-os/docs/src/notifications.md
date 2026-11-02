# Notifications

StartOS records system and service notifications on the **Notifications** page. Enable push in its settings to receive notifications while the StartOS UI is closed.

## Enable browser push

1. Open StartOS over HTTPS in a browser that supports Web Push. [Trust your server's Root CA](trust-ca.md) if needed.
2. Open **Notifications**, select the settings gear at the top right, and turn on **Push**.
3. Allow notifications when the browser asks.

Enable push separately in each browser or installed web app that should receive notifications. On iOS and iPadOS, add StartOS to the Home Screen and enable push from that installed web app. If notifications are blocked, allow them in the browser's site settings before trying again.

Turn off **Push** in the same settings modal to stop delivery to that browser. Opening a push notification takes you to the Notifications page; the complete message remains available there. StartOS retries temporary delivery failures for up to 24 hours.

## Self-hosted push services

StartOS uses the push subscription supplied by the browser, including a self-hosted UnifiedPush endpoint when the browser is configured to use one.

Push endpoints may use HTTP or HTTPS, including LAN addresses. StartOS encrypts notification content for the receiving browser before sending it to the endpoint. HTTPS also protects the endpoint URL and request metadata in transit. The endpoint must be reachable from your StartOS server through its [outbound traffic](outbound-traffic.md) configuration.
