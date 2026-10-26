- **DNS record publishing (RFC 2136).** A device with the new **Allow DNS record
  publishing** permission (off by default, on its device page) can publish DNS
  names into the router, pointing at any address, and every device on the
  network resolves them. A StartOS server uses it for its private domains. An
  inbound VPN client needs the same permission, granted from its VPN's client
  list. An update from the LAN must arrive over TCP (`nsupdate -v`). Names under
  `.lan` are refused, and a name is served only to networks whose Security
  Profile can reach the publishing device. The router drops a record whose
  device loses the address it published from, and revoking the permission
  removes the device's names immediately. A read-only table on the device page
  shows what a device has published. DNS record publishing coexists with DNS
  servers published on port 53, including access from the WAN and LAN.
- **Inbound VPN clients can publish DNS records.** A client added on an earlier
  release must be deleted and re-added before it can publish; its new
  configuration carries the address the router expects.
