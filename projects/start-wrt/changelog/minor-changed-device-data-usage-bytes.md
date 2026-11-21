---
release-notes: false
---

- `startwrt-cli devices list` and the `devices.list` RPC now report `data_usage` in whole bytes. To use the previous GB scale, divide each non-null value by 1,073,741,824 and round to one decimal place.
