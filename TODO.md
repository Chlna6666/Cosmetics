# Cosmetics staged validation

- [x] Initialize independent Rust \`cdylib\` project with no BLoader dependency
- [x] Implement exact host/path/method routing and JSON append/dedup engine
- [x] Keep original inventory values/receipts unchanged by default
- [x] Add WinHTTP synchronous request/response interception and local logs
- [x] Support local per-item PlayFab GetPublishedItem metadata response files
- [x] Add guarded config, empty template overlays and pure Rust unit tests
- [x] Log observed disabled endpoints without changing responses
- [x] Support optional full JSON replacement for custom storefront pages (not inventory)
- [ ] Run \`cargo test\` and \`cargo build --release\` on Windows MSVC
- [ ] Confirm Minecraft's actual HTTP stack and WinHTTP sync/async behavior
- [ ] Verify real DressingRoom_Capes JSON layout for each target version
- [ ] Obtain **sanitized** inventory schema and choose actual array pointer/key
- [ ] Add proper gzip/br support and raw header consistency if needed
- [ ] Handle WinHTTP asynchronous callbacks if game uses asynchronous mode
- [ ] Validate repeated reads, query headers, HTTP 404 catalog mock, failure replay
- [ ] Add opt-in local ZIP hosting or verified cache path, if required
- [ ] Confirm Persona signature / integrity / resource package acceptance
- [ ] Verify local cape previews, equipment, restarts, existing entitlement preservation
- [ ] Verify native injection with UWP and GDK, including early hook timing
- [ ] Validate skin selector / persona profile row schema with real responses
- [ ] Add a safe explicit shutdown procedure before ever supporting hot-unload

## Important constraints

No automatic CI code rewriting. Do not overwrite genuine owned cape
entries, mutate server receipts, or replace the complete original inventory.
Leave unrecognized responses alone. No assumption that a fake inventory
entry grants server-side ownership.
