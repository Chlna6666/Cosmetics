# Cosmetics

Version: **0.1.1 diagnostic build**. Built-in sample is intentionally NOT an equipable cape.

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

The release includes one **NotOwned test cape entry** by default to verify
the Dressing Room route. This is not a real wearable cape and does not grant
ownership. To disable the diagnostic item, set `overrides/capes_items.json`
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
