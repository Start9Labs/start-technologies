- **Firefox's default DNS-over-HTTPS steps aside.** Wherever the router serves
  published names or DNS overrides, its DNS answers Firefox's DNS-over-HTTPS
  canary domain, so Firefox's default-enabled DoH steps aside there and those
  names and overrides resolve. DoH a user turned on explicitly is untouched.
