---
user-facing: true
---

- A registry's `product` setting (`startos` or `startwrt`) names the operating system whose releases it serves. `startwrt: true` still selects StartWRT.
- A current StartOS server or StartWRT router that asks a registry for the other product's updates gets an error naming the product the registry serves.
- Clients that declare support for current signature labels receive them, whatever their product or version.
