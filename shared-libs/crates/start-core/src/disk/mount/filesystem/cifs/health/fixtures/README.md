# CIFS kernel snapshots

Captured from StartOS 0.4.1.2, Linux 7.2.11 and CIFS 2.61 against an isolated SMB3 Samba server. Addresses, client/session/tree identifiers and serial numbers are anonymized.

The snapshots cover a connected mount, server outage, recovery without remounting, and refused reconnection after password rotation. The authentication-failure snapshot retains a good TCP connection while the session and trees are unavailable.

These fixtures verify kernel-text parsing, not StartOS callback or consuming-service behavior.
