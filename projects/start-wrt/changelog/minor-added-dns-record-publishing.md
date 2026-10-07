- **DNS record publishing (RFC 2136).** A device with the new **Allow DNS record
  publishing** permission (off by default, on its device page) can publish DNS
  names for itself into the router, and every device on the network resolves
  them. A StartOS server uses it for its private domains, and one joined over
  the inbound VPN publishes without the toggle, which also makes its `.local`
  name resolve for VPN devices. An update from the LAN must arrive over TCP
  (`nsupdate -v`). Names under `.lan` are refused, and a name is served only to
  networks whose Security Profile can reach the publishing device. The router
  drops a record whose device loses the address it published from, and revoking
  the permission removes the device's names immediately. A read-only table on
  the device page shows what a device has published.
