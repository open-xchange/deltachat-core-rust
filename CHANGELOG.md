# Changelog 

## 1.0.0-beta2

- https://c.delta.chat docs are now regenerated again through our CI 

- several rPGP cleanups, security fixes and better multi-platform support 

- reconnect on io errors and broken pipes (imap)

- probe SMTP with real connection not just setup

- various imap/smtp related fixes

- use to_string_lossy in most places instead of relying on valid utf-8
  encodings
 
- rework, rustify and test autoconfig-reading and parsing 

- some rustifications/boolifications of c-ints 


## 1.0.0-beta1 

- first beta of the Delta Chat Rust core library. many fixes of crashes
  and other issues compared to 1.0.0-alpha.5.

- Most code is now "rustified" and does not do manual memory allocation anymore. 

- The `DC_EVENT_GET_STRING` event is not used anymore, removing the last
  event where the core requested a return value from the event callback. 

  Please now use `dc_set_stock_translation()` API for core messages
  to be properly localized. 

- Deltachat FFI docs are automatically generated and available here: 
  https://c.delta.chat 

- New events ImapMessageMoved and ImapMessageDeleted

For a full list of changes, please see our closed Pull Requests: 

https://github.com/deltachat/deltachat-core-rust/pulls?q=is%3Apr+is%3Aclosed
