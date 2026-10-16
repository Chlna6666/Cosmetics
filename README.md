# Cosmetics

Version: **0.1.11**. Two Cosmos CDN-backed cape metadata presets are included,
with **NotOwned** ownership. Their actual Persona ZIPs are **not embedded**
or republished. The older manifest-only diagnostic ZIP is still available as
an opt-in protocol probe.

Minecraft Bedrock (Windows) standalone native **Rust DLL** for cosmetic dressing-room
HTTP response overlays. This is a research build, not a proven cape unlocker.

**Not a BLoader mod:** No BLoader API is linked or used. BMCBL can inject
`Cosmetics.dll` through its independent native DLL injection path. The DLL only
uses Windows WinHTTP/MinHook, JSON files, and logs; it never reads ClientInstance
or LocalPlayer and does not patch Minecraft's EXE.

## Goals

- Display custom entries alongside the game's original dressing-room capes.
- Allow the original character creator / skin selector to be extended with JSON
  rules, without overwriting official page data.
- Resolve *locally registered* PlayFab catalog item IDs to test Persona package
  metadata containing a `personabinary` ZIP URL.
- Preserve every existing inventory entitlement, receipt and response field;
  use append-only, deduplicated overlays. Never send account data to a third
  party. Never overwrite or delete the official premium cache.

## Build

On Windows with Rust (MSVC toolchain + Visual Studio C++ build tools installed):

    cargo test
    cargo build --release

Output: `target/release/cosmetics.dll`.

`cosmetics.json` is also compiled into the DLL as a read-only default.
The local configuration file, when present, takes precedence.

Copy `cosmetics.dll`, `cosmetics.json`, and the `overrides/` folder
into the same native-mod directory. Inject the DLL using BMCBL's **native** DLL injection facility. This DLL does not use any
BLoader API. **Important:** BMCBL's current `preload-native` launcher path
may still *transport/load* the DLL through BLoader. If the injection path itself
must be BLoader-free, BMCBL needs a separate early direct-injection route;
a late `hot-inject` may miss marketplace requests made at startup. The DLL starts a worker thread
outside DllMain and writes `logs/cosmetics.log` beside itself.

A sample native Mod manifest is provided in
`examples/bmcbl/manifest.json`. This is only the package format: it does
not alter the launcher's existing injection transport.

No proxy, certificate installation, backend HTTP service or DLL-specific
network connection is needed. Ordinary game HTTPS traffic uses the original TLS
stack and the module observes/modifies **plaintext at WinHTTP API boundaries**.

## Supported response rules

All rules live in `cosmetics.json`, matching host, exact path (including
query string), and HTTP verb. Only active rules are processed.

| Route | Default | Semantics |
| --- | --- | --- |
| `DressingRoom_Capes` | active | Append unique entries to the selected cape list; don't replace official items |
| `DressingRoom_PersonaProfile` | inactive | Append menu rows (requires schema-specific sample) |
| `MultiItemPage_PersonaSkinSelector` | inactive | Append classic-skin rows (requires schema-specific sample) |
| `MultiItemPage_PersonaCharacterCreator` | inactive | Append Persona rows (requires schema-specific sample) |
| `PersonaCreatorPage_master_player_account!CustomCapes` | inactive | Optional, complete version-compatible custom page JSON |
| `/api/v1.0/player/inventory?includeReceipt=true` | inactive | **Append only** after verifying actual response schema |
| `/Catalog/GetPublishedItem` | active | Override responses for exact, explicitly registered local item IDs |
| `/Catalog/Search` | inactive | Experimental catalog list extension |

The release includes two **NotOwned Cosmos cape entries** by default to verify
the Dressing Room route: Android Cape and Minecon 2011 Cape. Both reference
the Cosmos asset CDN, and do not grant ownership. A separate manifest-only
diagnostic ZIP is retained, but not added to the default cape menu. To disable the diagnostic item, set `overrides/capes_items.json`
to `[]`. Read [docs/DIAGNOSTICS.md](docs/DIAGNOSTICS.md) for expected logs.

Array overlay files contain a JSON array. Empty arrays mean **no change**.
Append mode requires an existing JSON array at the configured JSON Pointer.
If the structure changed, the module *replays the original body* instead of
replacing it with empty/default data. For duplicate identifiers it preserves the
original item, including any official entitlement and receipt.

Disabled response rules are still observed and logged, without altering the response.
A `replace_json` rule can serve a complete custom storefront page but is never
used on the user's entitlement inventory.

The inventory `/result/items` path in the sample config is deliberately a
**placeholder, not a verified schema**. The inventory hook is disabled until a
sanitized, version-specific response establishes the real array location and
unique item identity. Do not blindly enable it or attempt to rewrite receipts.

`/Catalog/GetPublishedItem` uses `itemid` from the request body to read only
`overrides/catalog/<uuid>.json`. The local file must have the same
`data.Item.Id`. Requests for any other item pass through untouched.

For ordinary custom classic skins, prefer Bedrock's native `skin_packs` folder;
it does not require network interception. For standalone Persona capes, the game
must still accept the referenced ZIP package and its integrity checks. The
response hook alone does **not** bypass signatures or server-side ownership.

## Current technical boundary

This implementation hooks **WinHTTP synchronous requests and asynchronous status callbacks**:
WinHttpOpen, WinHttpConnect, WinHttpOpenRequest, WinHttpSendRequest,
WinHttpWriteData, WinHttpReceiveResponse, WinHttpReadData,
WinHttpQueryDataAvailable, WinHttpQueryHeaders, WinHttpSetStatusCallback and WinHttpCloseHandle.

On matching responses, it reads the original body (up to 4 MiB by default),
produces a local JSON overlay, then replays that body via original WinHTTP
read APIs. For custom locally registered catalog IDs, it can return HTTP 200
metadata even if the original catalog response was not successful.

A JSON response may be gzip, deflate or brotli encoded. The hook applies a
bounded decoder and exposes modified responses as uncompressed data. Untouched
responses retain their original bytes and headers. Native callback and header
behavior still requires target-version testing.

**Virtual Persona resource URL:** Place a pre-existing Persona ZIP named
`<resource-uuid>.zip` under `overrides/persona/`, and register its corresponding
PlayFab catalog record with `Contents[].Url` set to
`https://cosmetics.invalid/personabinary/<resource-uuid>.zip`.
The DLL answers requests for this reserved name entirely inside WinHTTP, without
sending a DNS request or touching official premium_cache/persona. The packaged probe ZIP contains a minimal plaintext manifest and **no
actual cape**. This only tests local ZIP delivery, not game equipability.
A real ZIP
must contain a plaintext `manifest.json` at the ZIP root and a
`persona_piece` module. **Successful delivery is not proof that Minecraft
accepts the package or allows it to be equipped.**

The module intentionally **does not handle**:
- Other HTTP stacks or WinHTTP sessions opened before native DLL injection.
- Unverified callback registration patterns that do not use WinHttpSetStatusCallback.
- All forms of header enumeration / raw HTTP response headers; applications
  that use raw headers require further implementation.
- Producing *valid signed* Persona binaries or translating proprietary skin formats.
  A local Persona ZIP can now be supplied as opaque bytes from overrides/persona.
  In-game accept/equip remains subject to the game's own validation.
- Any entitlement signature bypass, genuine Microsoft inventory modification,
  offline marketplace impersonation or guarantees about multiplayer visibility.

These limits mean no claim of end-to-end Minecraft compatibility can be made
until tested on specific UWP and GDK versions. A completely unrecognized
WinHTTP flow passes through to Minecraft unchanged.

## Safe validation sequence

1. Start in an isolated copy of the game with all overlay arrays empty.
   Check `logs/cosmetics.log` for successful WinHTTP hook installation.
2. Navigate into the original dressing room and verify logging for target
   routes. If nothing appears, inspect the game's actual HTTP stack and
   injection timing; **do not assume WinHTTP is used**.
3. Insert one unique test cape item into `overrides/capes_items.json` and
   verify the list still includes *all* original items.
4. Add a matching catalog JSON and a legitimate test ZIP URL, then check
   downloads, `premium_cache/persona`, and cache reuse without editing
   the user's official cache.
5. After observing the real official inventory schema, update the exact
   JSON pointer and identifier key, enable its overlay and verify that
   existing owned capes and receipts remain byte-equivalent as JSON values.
6. Test preview, selection, equip, restart, reconnect and UWP/GDK separately.

Never log Authorization, MCToken, account IDs, complete response JSON or
receipts. Windows CI builds an x64 package and uploads it as an Actions artifact;
CI cannot replace UWP/GDK tests on the actual game.

## License

GPL-3.0-only. This repository retains its existing GPL-3.0 license.

## Cosmos resource integration

Cosmetics follows the response schema of [Bedrock.Cosmos](https://github.com/BE-Community-Dev/Bedrock.Cosmos/)
and the factual item-ID / Persona UUID / ZIP URL relationships of
[Bedrock-Cosmos/Responses](https://github.com/Bedrock-Cosmos/Responses).

The package and DLL embed **two minimal Cosmos-compatible metadata templates**,
not third-party binary assets or wholesale copies of the upstream response database.
When an external `overrides/capes_items.json` or matching catalog JSON is
present it takes precedence; otherwise the DLL uses its embedded metadata.
Using `[]` in the external capes file disables the two presets. The
source repositories do not expose a clear repository-wide license for
redistributing their complete response collection and cape ZIP bytes.

For a larger *local-only* collection, use the bundled opt-in importer:

```powershell
py scripts/import_cosmos.py --destination . --limit 15
```

This imports selected Cosmos cape metadata and PlayFab item responses, sets
imported ownership to `NotOwned`, and preserves the user's existing entries.
To download ZIPs into `overrides/persona` for your private environment,
there is a separate rights-confirmed option. Read
[docs/COSMOS_IMPORT.md](docs/COSMOS_IMPORT.md).

**Local metadata is not a server entitlement.** Neither importer nor DLL
modifies the real inventory, receipts, or Minecraft's existing Persona cache.

## v0.1.3 capture diagnostics

The DLL prints the real Cargo version at startup. Under `logs/cosmetics.log`, every
recognized commerce request reports a redacted route and whether an overlay was
actually installed. Unknown commerce paths are grouped into non-sensitive
endpoint families (query strings and page identifiers never leave the process).
`Catalog/GetPublishedItem` now recognizes case-insensitive `ItemId`, `Id`
and selected nested object forms; requests not found in the local catalog still
pass to the server unchanged. `client_read_complete` confirms the game's
WinHTTP consumer read the entire synthetic entity, not that Minecraft accepted
its Persona content.

The PersonaProfile screen now has a dedicated optional navigation tile to
`DressingRoom_Capes`, integrated using the upstream Cosmos `StoreRow` / dropdown
0 structure. Disable `persona_profile` in cosmetics.json to suppress it. Existing
owned items and inventory are not replaced.

## v0.1.4 development capture trace

Development tracing is enabled by default:

```json
"debug": {"verbose": true, "response_shape": true}
```

The module writes `logs/cosmetics.log` beside the DLL and emits
`[Cosmetics DEBUG]` through Windows `OutputDebugStringW` (DebugView / Visual
Studio debugger output). No BLoader, TLS proxy, console or certificate is needed.

Events include `REQUEST_CAPTURED`, `SEND_RESULT`, `RESPONSE_HEADERS_CAPTURED`,
`RESPONSE_CAPTURED`, `UPSTREAM_JSON`, `RESPONSE_REPLACED`, `MODIFIED_JSON`,
`CLIENT_CONSUMED_OVERLAY`, and `HANDLE_CLOSE` for matched API requests.

JSON debug summaries show aggregate layout counts, item-list counts, lengths
and whether `data.Item` exists. They deliberately do not include original JSON,
account IDs, tokens, inventories, receipts, or unredacted URLs. `RESPONSE_REPLACED`
confirms the DLL generated response bytes, while `CLIENT_CONSUMED_OVERLAY` confirms
WinHTTP delivered them, **not** that Minecraft accepted a cape as equipable.

Set `debug.verbose=false` for less logging, or `debug.response_shape=false`
to avoid additional development-only JSON inspection costs.


## v0.1.5 Bedrock 26.40 storefront POST correction

Real Bedrock 26.40 WinHTTP traces show that the game issues
`POST /api/v2.0/layout/pages/DressingRoom_Capes` and
`POST /api/v2.0/layout/pages/DressingRoom_PersonaProfile`.
Previous Cosmetics releases expected GET, so the hook did not apply any
replacement despite observing those exact page paths. Storefront routes now
allow `GET,POST` while PlayFab and inventory endpoints remain verb-specific.
Official entitlements and receipts are still untouched.

`scripts/import_cosmos.py --destination . --all` (or `--limit 0`) imports
every mapped cape from the current upstream page, with parallel catalog checks.
On the inspected Cosmos revision, 270 page capes have 245 mapped catalog
records; the other 25 have no local mapping and remain excluded by default.
An explicit `--include-unmapped` adds those display-only items, but does not
make their Persona ZIPs available or grant ownership.

The importer adds to existing files and prints verification progress. Update
both `Cosmetics.dll` and `cosmetics.json` from the same 0.1.5 artifact.
## 0.1.6 Cosmos-style official cape client-side preview

Bedrock.Cosmos implements three separate parts: custom Dressing Room page
responses, local PlayFab catalog response substitution, and a response from
its **closed, separate website API** for Minecraft player inventory. The
public code for the last part simply POSTs the original inventory body to
`https://bedrock-cosmos.app/api/v1.0/player/inventory?includeReceipt=true`.
This repository does not contact that service, forward private inventory
JSON or attempt to mint authenticated purchase receipts.

**Opt-in local official cape ownership display:**

```powershell
py scripts/import_cosmos.py --destination . --all --official-preview
```

This imports mapped Cosmos items and creates
`overrides/official_cape_preview_ids.json`. It allows the DLL to mark only
allowlisted `Minecraft` creator + `persona_capes` items as `Purchased`
**in the local Dressing Room HTTP page only**. No original inventory,
receipt, signature, user entitlement or external server state changes.
Existing items retain every other field. Creator capes are left unchanged.
An empty or missing allowlist switches the UI ownership projection off.

**Important:** this is a client-side owned-state experiment, not a genuine
official cape purchase or a guarantee of preview/equipment/multiplayer
visibility. The game may independently verify entitlement and package
signatures and reject the local wardrobe state.

An optional read-only `official_inventory_schema_probe` rule is present,
**disabled by default**. It records aggregate JSON shape without changing
the server response; it helps analyze the unknown inventory API schema.
Turning it on buffers inventory data locally and increases overhead. It
does not send data to Cosmos or any other third-party service.

Source implementation references:
- BedrockCosmos.Core/Proxy/ProxyController.cs: OnResponse, HandleEntitlementsRequest
- BedrockCosmos.Core/App/AsyncHttpOperations.cs: GetCosmosEntitlementsAsync
- Bedrock-Cosmos/Responses/MainPages/Capes.json
## v0.1.7 — completely local, standalone cape ZIPs

Put each user-authored ZIP in **`capes/` beside `Cosmetics.dll`**. That's all.
No separate catalog JSON, `capes_items.json`, Cosmos importer, XBL inventory
overlay, external CDN or static resource deployment is needed for these packs.
The DLL scans this folder at game startup. After adding/removing ZIPs, restart.

At minimum a ZIP contains `cape.png` (without nested folders).
Optionally include `cape.json` with `name`, `author` and fixed `id` (UUID).
The ZIP can instead contain a prebuilt `persona.zip`, optionally with a
`cape.png` for the thumbnail. The DLL never extracts these archives to disk.

```powershell
py scripts/pack_cape.py my_cape.png --name "My Cape" --author "Player"
```

The helper writes `capes/My Cape.zip` and embeds a tiny `cape.json` automatically.
Internal game-specific item IDs, catalog JSON, Persona resource URL and PNG
thumbnail are generated entirely in memory. The older `overrides/` folder is
unrelated and remains only for compatibility with official/Cosmos workflows.

**Important:** PNG-only ZIPs are converted to an experimental plaintext
Persona-shaped ZIP (`manifest.json`, `contents.json`, `textures/cape.png`).
This is a transport prototype: game acceptance and wearing are unverified;
the game may require more Persona metadata, real signatures or other runtime
state. A user-supplied valid native `persona.zip` is transmitted unchanged.
Neither variant grants real Microsoft entitlements or multiplayer visibility.

See [docs/LOCAL_CAPES.md](docs/LOCAL_CAPES.md).

## v0.1.8: WinHTTP callback isolation and marketplace safety

Minecraft 26.40 diagnostics show `Catalog/Search` returning HTTP 401 while its
Cosmetics rule is disabled. That is an upstream authorization status, not
evidence that Cosmetics deliberately rewrote the HTTP body. However older
Cosmetics builds attached their WinHTTP status callback bridge to *all*
sessions and connections, including disabled/irrelevant marketplace endpoints.

v0.1.8 keeps the game's original callbacks and flags unchanged for every
unrelated or disabled request. The bridge is now attached only to the
individual request handles for active rules and local virtual cape assets.
Non-target SendRequest, WriteData and ReceiveResponse calls go straight
to the original functions, and non-target handle state is cleaned on close.

### Marketplace A/B isolation test

To test whether an unexplained upstream HTTP 401 occurs without Cosmetics
network interception, set ``"enabled": false` at the ROOT of
`cosmetics.json`, fully exit the game and restart. A successful safe-mode
startup prints `SAFE_MODE: cosmetics.json enabled=false; WinHTTP hooks NOT installed`.
This leaves Cosmetics loaded for diagnosis but performs no WinHTTP hooks at all.
If marketplace search still gets HTTP 401, investigate token/authentication/
server behavior instead of assuming cape JSON caused it. Re-enable only after
you have confirmed the baseline.

### Cape loading result is now explicit

`ZIP_SCAN complete discovered=N validated=N rejected=N ...` means local pack
files were validated and registered in memory. It does NOT say Minecraft
accepted their Persona payloads. `in_game_loaded=UNVERIFIED` is intentional;
look for `DressingRoom_Capes` response modification and local resource requests.

Even if marketplace authorization is restored, a generated plaintext Persona
ZIP is experimental and may still be rejected by the game.

## v0.1.9 — unambiguous A/B local-only test

Two configuration profiles are included in the Windows ZIP artifact:

- `cosmetics.safe-mode.json` — root `enabled=false`, WinHTTP hooks are never installed.
- `cosmetics.local-only.json` — ONLY the two required route hooks, with
  `local_only=true`. `DressingRoom_Capes` appends only memory-registered
  `capes/*.zip` products; `GetPublishedItem` recognizes only local ZIP IDs.
  Neither rule reads imported Cosmos catalog files, ownership allowlists,
  or built-in Cosmos product presets.

Back up `cosmetics.json`, then copy ONE profile to `cosmetics.json`, close
Minecraft completely, and relaunch. `rule=... local_only=true` in the log
confirms the minimal profile. If the only observed requests are
`Catalog/Search (disabled)` or unmatched requests, the game has not issued
the wardrobe/catalog requests and it is impossible for a page overlay to
appear. A previous 401 from PlayFab is an upstream authorization response
and must be diagnosed independently; do not rewrite it to a synthetic 200.

IMPORTANT: even in local-only mode, the local cape *appearance in Minecraft*
still depends on the game requesting the `DressingRoom_Capes` page and
successfully parsing the returned response; no native wardrobe-internal
injection has been implemented.

## v0.1.10 — Cape thumbnails and Persona identity consistency

The previous PNG-only pipeline reused the full 64x32 cape UV sheet as the store
thumbnail. It now actually decodes PNG images, validates decoded content, and
creates a 64x64 thumbnail from the visible cape back (12,1 to 22,17 in a
64x32 UV texture). Optional native `persona.zip` remains unchanged.

Persona `packIdentity.uuid` must refer to the pack **manifest header.uuid**,
not the separate `modules[].uuid`. Native packs use the header UUID and
version from their own manifest. PNG-generated test ZIPs now use a deterministic
header UUID consistently in both their manifest and catalog records.

Local items now carry the same zero-price data fields used in the Cosmos
wardrobe catalog. Bundled legacy Cosmos catalog records also use the public
`purchasable` and `originalCreatorId` fields from their upstream metadata,
without claiming ownership.

New traces: `LOCAL_ASSET_REQUEST`, `LOCAL_ASSET_RESPONSE_READY`,
`CATALOG_RESPONSE_READY source=local_zip|legacy_or_cosmos`. On clicking a cape,
collect the log starting at the click through its failure. These traces
can tell whether the game fetched the catalog entry, tried to download the
Persona package, and consumed its bytes. They **cannot** prove equip success
because equipping may be performed inside Minecraft without a WinHTTP request.

### Important protocol limitation

Actual Persona cache packages observed in Bedrock 26.40 contain distinct
manifest header/module UUIDs, a binary `contents.json` and signature metadata.
The PNG-only builder currently produces a plaintext **experimental** ZIP, not
a verified, signed game-native Persona package. It can be displayed in the
wardrobe but may fail to load, preview, or equip. The two bundled Cosmos
items remain `NotOwned` storefront examples pointing to third-party CDN ZIPs;
they are not authenticated account inventory and are not expected to equip
without the game's own acceptance/entitlement path. No Xbox inventory or
Microsoft entitlement state has been altered.

Use `cosmetics.local-only.json` from the build to isolate custom ZIPs from
the two Cosmos metadata examples and existing imported override files.

## v0.1.11 — independent cape categories, matching Cosmos page layout

Cosmos' `MainPages/Capes.json` actually defines sections for owned, Vanilla,
Spin-Off, Bedrock Cosmos, Custom, Creator and Skin Pack capes. The previous
Cosmetics implementation appended all 3 sources into one list and obscured
their provenance.

The Cosmetics DressingRoom_Capes overlay now clones a compatible **existing**
`GridList` + `headerComp` + `itemListComp` + preview component and creates
separate rows when those sources exist:

- `Official Capes - Catalog Only`: newly imported capes attributed to
  Minecraft/Minecraft Dungeons. This is a **catalog classification**, not
  an unlock or authenticated account entitlement.
- `Cosmos & Community Capes`: other legacy/Cosmos catalog entries.
- `My Local ZIP Capes`: self-authored `capes/*.zip`, never mixed into
  a third-party/official catalog section.

The original Owned capes grid and all existing rows remain intact. Missing
categories are omitted instead of creating blank rows. IDs are deduplicated
against all existing item grids. A section is created from the game's page
components, not from a second hardcoded whole-page response. No extra files,
IDs, or user config required.

Display categorization never changes cape equipment permissions. Only the
existing **explicit opt-in** official preview allowlist can change an
individual `ownership` field in the HTTP page.

Note: original Cosmos may contain other distinctions such as Spin-Off/Skin
Pack, but the three new Cosmetics source groups are not a claim that
these capes have actually been unlocked or equipped.
