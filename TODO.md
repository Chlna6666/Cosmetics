# Cosmetics staged validation

- [x] Initialize independent Rust \`cdylib\` project with no BLoader dependency
- [x] Implement exact host/path/method routing and JSON append/dedup engine
- [x] Keep original inventory values/receipts unchanged by default
- [x] Add WinHTTP synchronous request/response interception and local logs
- [x] Support local per-item PlayFab GetPublishedItem metadata response files
- [x] Add guarded config, empty template overlays and pure Rust unit tests
- [x] Embed a read-only default config for standalone DLL injection
- [x] Add catalog item match and unknown UUID tests
- [x] Log observed disabled endpoints without changing responses
- [x] Support optional full JSON replacement for custom storefront pages (not inventory)
- [ ] Run \`cargo test\` and \`cargo build --release\` on Windows MSVC
- [ ] Confirm Minecraft's actual HTTP stack and WinHTTP sync/async behavior
- [ ] Verify real DressingRoom_Capes JSON layout for each target version
- [ ] Obtain **sanitized** inventory schema and choose actual array pointer/key
- [ ] Add proper gzip/br support and raw header consistency if needed
- [ ] Handle WinHTTP asynchronous callbacks if game uses asynchronous mode
- [ ] Validate repeated reads, query headers, HTTP 404 catalog mock, failure replay
- [x] Log sanitized matched requests, captured request body sizes, and per-response action
- [x] Add a non-equipable diagnostic cape item, catalog metadata and manifest-only ZIP
- [ ] Confirm status callback, request body and manifest-only ZIP delivery using Minecraft
- [ ] Add opt-in local ZIP hosting or verified cache path, if required
- [ ] Confirm Persona signature / integrity / resource package acceptance
- [ ] Verify local cape previews, equipment, restarts, existing entitlement preservation
- [ ] Verify native injection with UWP and GDK, including early hook timing
- [ ] If required, implement a BMCBL early direct-injection route independent of BLoader transport
- [ ] Validate skin selector / persona profile row schema with real responses
- [ ] Add a safe explicit shutdown procedure before ever supporting hot-unload

## Important constraints

No automatic CI code rewriting. Do not overwrite genuine owned cape
entries, mutate server receipts, or replace the complete original inventory.
Leave unrecognized responses alone. No assumption that a fake inventory
entry grants server-side ownership.

## Cosmos metadata integration (0.1.2)
- [x] Verify upstream Cosmetics/Cosmos page and catalog metadata schema
- [x] Update itemListComp totalItems and customStoreRowConfiguration.maxOffers on append
- [x] Support explicitly allowlisted Cosmos HTTPS ZIP metadata without a local proxy
- [x] Package two minimal NotOwned Cosmos item mappings (Android/Minecon 2011)
- [x] Provide an opt-in local resource/mapping importer; don't redistribute unknown-license upstream ZIPs
- [ ] Verify original Minecraft UI navigation / Persona equipment with real packages
- [ ] Confirm downloaded ZIP signatures, accepted manifest format, and cache reuse in both UWP/GDK
- [ ] Fix and validate async callback lifecycle under concurrent independent WinHTTP clients

## v0.1.4 debug capture

- [x] Verbose and response_shape configuration switches (on by default in development)
- [x] Emit request lifecycle diagnostics to file and Windows debugger
- [x] Separate upstream capture, JSON overlay creation and client consumption
- [x] Summarize JSON shape without leaking account inventory and credentials
- [ ] Validate lifecycle and Persona acceptance against Bedrock UWP/GDK


## v0.1.5
- [x] Accept GET/POST for Bedrock 26.40 storefront layout pages
- [x] Preserve server-advertised list capacity when appending many capes
- [x] Fix WinHTTP Transfer-Encoding header constant to 63
- [x] Add unbounded importer mode and parallel metadata fetch with progress
- [ ] Verify Bedrock 26.40 POST page JSON schema against live response
- [ ] Verify end-to-end client read and actual Persona equipability
## v0.1.6 local official wardrobe
- [x] Inspect Cosmos entitlement forwarding and separate remote backend boundary
- [x] Add opt-in verified official item ID list for local-owned UI projection
- [x] Keep original non-ownership item fields and existing inventory receipts intact
- [x] Add read-only opt-in original inventory schema probe
- [ ] Verify actual game equipment, signatures and persistence after restarting the client
- [ ] Derive actual inventory schema from consented redacted fixture before any local entitlement overlay

## v0.1.7 standalone cape ZIP pipeline
- [x] Scan capes/*.zip without adding more cosmetics.json routes
- [x] Auto-generate stable item/resource UUIDs, catalog JSON and cape page items
- [x] Serve self-contained virtual thumbnails and Persona-shaped ZIPs
- [x] Accept optional original native persona.zip without modifying bytes
- [x] Keep Cosmos/official overrides completely separate
- [ ] Verify whether 26.40 accepts the experimental PNG-to-Persona payload
- [ ] Implement validated game-native Persona binary authoring when the real schema is known
- [ ] Verify selection, equip persistence and multiplayer visibility in-game

## v0.1.8 Marketplace isolation
- [x] Keep original WinHTTP status callback and notification flags on disabled/unrelated requests
- [x] Install async callback bridge only on specifically enabled request handles
- [x] Bypass original Send/Write/Receive wrappers on unrelated requests
- [x] Add `enabled=false` no-hook safe mode for A/B reproduction
- [x] Split ZIP scanning/registration from unverified Minecraft acceptance in logs
- [ ] Validate PlayFab 401 A/B with Cosmetics hooks completely disabled
- [ ] Verify target async callback inheritance and status event ordering in Minecraft 26.40
- [ ] Verify native Persona equip while original marketplace is healthy
