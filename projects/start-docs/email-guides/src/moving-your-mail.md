# Moving Your Mail

Bring your existing mail to a new provider or your own server, and run your old and new addresses side by side while everyone catches up.

## Copying Mail Between Accounts

The simplest way to move mail is through Thunderbird, which can hold both accounts at once:

1. Add your old account and your new account to Thunderbird, both over IMAP.
1. Let the old account finish downloading. In the old account's settings, under **Synchronization & Storage**, keep its messages on this computer so every folder downloads in full.
1. Create matching folders in the new account, then select the messages in each old folder, right-click and choose **Copy To**, and select the matching new folder. Ordinary dragging moves messages and removes them from the old account.
1. Check the message counts in each folder match before you delete anything from the old account.

Thunderbird uploads each copied message to the new server, so a mailbox of many gigabytes takes hours. Leave it running.

For very large mailboxes, or many accounts at once, **imapsync** copies between two IMAP servers directly and can resume after an interruption.

### Gmail

Gmail's labels appear as folders over IMAP, and a message with several labels appears in each of them. Copy **All Mail** if you want everything exactly once, or copy label folders if you want your organization preserved and don't mind duplicates.

For an offline copy instead, [Google Takeout](https://takeout.google.com/) exports your mail as an `.mbox` file, which Thunderbird can import with the ImportExportTools NG add-on.

## Moving an Address You Already Receive Mail At

If your domain already receives mail somewhere, the switch happens when you change its MX record:

1. Set up the new provider or server completely, including mailboxes, before touching the MX record.
1. Copy your existing mail across, as above.
1. Change the MX record, and the SPF, DKIM and DMARC records, to the new values.
1. Keep the old mailbox for a week. Some senders will still deliver there until DNS changes spread, so check it, and copy across anything that arrived late.

Mail is not lost during the switch: each sender delivers to whichever server its DNS lookup returns at the time.

## Changing to a New Address

Moving from an address you don't own, like `you@gmail.com`, to one at your own domain can't happen all at once. Run them side by side:

- **Forward the old address to the new one.** Gmail, Outlook.com and most providers can forward incoming mail automatically, so you only need to check one mailbox.
- **Update your accounts gradually.** Change your address at banks, shops and services as you next use them, starting with the ones that matter most — anything used to reset passwords.
- **Tell people as you reply.** Answer from your new address; most correspondents switch on their own.
- **Keep the old account open.** Leave it forwarding for a year or more, so nothing sent to it is lost and you can still reset any account you forgot to update.
