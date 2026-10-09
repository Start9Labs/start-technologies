# Notifications

Start9 Support notifies you on each device you turn it on for, whenever Start9 replies in one of your chats — whether or not the app is open. This page covers turning notifications on, and what to do when they don't arrive.

## What You're Notified Of

You are notified of every reply in your own chats, from Dux or from a person, as soon as it is posted — unless you have that chat open in front of you at the time. A notification says who replied, not what they wrote; tap it to open the chat.

If notifications can't reach you, [unread-reply reminders](email.md) by email can.

## Turning Them On

When you open Start9 Support on a device that isn't notified yet, it offers to turn notifications on; choose **Turn on** and allow them when your browser asks. To do it later, open **Settings** from the account menu, under your name at the bottom of the chat list. Its **Notifications** section shows whether this device is notified, and offers **Turn on** when it isn't.

A bell with a line through it beside your name means this device isn't being notified; Settings says why.

**Push notifications**, also in Settings, turns notifications off on every device at once, and back on.

## When Notifications Don't Arrive

Open **Settings** on the device that should be notified, and find what its **Notifications** section says:

### Notifications Are Off

This device hasn't been asked yet. Choose **Turn on**, and allow notifications when the browser asks.

### Add Support to Your Home Screen

On iPhone and iPad, only the installed app can receive notifications. [Install it](installing-the-app.md), open **Support** from your Home Screen, sign in there, and turn notifications on.

### Notifications Are Blocked

Notifications were refused for support.start9.com on this device.

{{#tabs global="platform"}}
{{#tab name="iOS"}}

Open the **Settings** app, then **Notifications**, then **Support**, and allow notifications.

{{#endtab}}
{{#tab name="Android / Graphene"}}

In your browser's site settings for support.start9.com, allow notifications, then come back. If you installed the app, also check that notifications are allowed for it in Android's app settings.

{{#endtab}}
{{#tab name="Mac"}}

In your browser's site settings for support.start9.com, allow notifications, then come back. Also check that the browser itself may notify you, in System Settings under **Notifications**.

{{#endtab}}
{{#tab name="Windows"}}

In your browser's site settings for support.start9.com, allow notifications, then come back. Also check that the browser itself may notify you, in Windows Settings under **System**, then **Notifications**.

{{#endtab}}
{{#tab name="Linux"}}

In your browser's site settings for support.start9.com, allow notifications, then come back.

{{#endtab}}
{{#endtabs}}

### This Browser Can't Receive Notifications

The browser has no push service to deliver notifications through. On Android, browsers deliver them through Google Play services, which GrapheneOS and other de-Googled systems don't have unless you install them. There are two ways to get notifications without them:

- **Install sandboxed Google Play** (GrapheneOS), and use a browser that delivers through it, such as Chrome or Firefox.
- **Use UnifiedPush.** Fennec and IronFox can deliver notifications through UnifiedPush instead:
  1. Install a UnifiedPush distributor app, such as ntfy.
  1. Turn on UnifiedPush in the browser's settings (in IronFox, **Use UnifiedPush**), and restart the browser.
  1. Allow notifications for support.start9.com in the browser's site settings, then open Start9 Support and choose **Turn on**.

A distributor delivers only to apps and browsers that use UnifiedPush; installing one alone changes nothing.

### This Browser Can't Show Notifications

The browser doesn't support web notifications. Use a recent version of Chrome, Edge, Firefox or Safari.

### We Couldn't Turn On Notifications

Something went wrong while turning them on. Choose **Try again**.

### Notifications Are On, but Nothing Arrives

- Notifications are held back for a chat you have open in front of you.
- Check that your phone isn't in a Do Not Disturb or Focus mode, and that battery saving isn't stopping the browser or app from running in the background.
- A device is notified only of replies posted after it was turned on.
