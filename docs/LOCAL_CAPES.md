# Cosmetics Local Capes — standalone ZIP packs

This feature is independent from \`overrides/catalog\`, \`capes_items.json\`,
the Cosmos importer, and Microsoft inventory. The DLL automatically creates
the \`capes\` directory next to itself and scans \`capes/*.zip\` once at startup.

## 1. Make a cape in under a minute

Create a PNG cape image using any editor. Keep it a PNG, ideally with
a conventional cape aspect ratio. From the folder beside Cosmetics.dll:

\`\`\`powershell
py scripts/pack_cape.py my-cape.png --name "My First Cape" --author "Me"
\`\`\`

This makes \`capes/My First Cape.zip\`; no manual UUID, catalog or JSON editing.
You can also zip just \`cape.png\` into \`capes/Blue.zip\` with File Explorer.
The displayed name defaults to "Blue" and author to "Player".

## 2. Optional cape.json

A ZIP may contain:

\`\`\`text
my-cape.zip
├── cape.png
└── cape.json
\`\`\`

Inside \`cape.json\`:

\`\`\`json
{
  "name": "My Custom Cape",
  "author": "Player",
  "id": "11111111-2222-4333-8444-555555555555"
}
\`\`\`

**All fields are optional.** \`id\` is useful for keeping identity unchanged
across filename/name edits. Without it, a deterministic UUID is calculated
from filename, name and author. The game-specific resource UUID is generated
from the item ID, without extra configuration.

## 3. Optional original native Persona ZIP

If you already have a valid \`persona.zip\` that includes \`manifest.json\`
with a \`persona_piece\` module, you can place it inside your personal pack:

\`\`\`text
my-cape.zip
├── cape.json       (optional)
├── cape.png        (optional thumbnail)
└── persona.zip     (native Persona bytes, not modified)
\`\`\`

Or use:

\`\`\`powershell
py scripts/pack_cape.py my-cape.png --persona-zip original-persona.zip
\`\`\`

The DLL serves this existing native payload byte-for-byte through the game's
WinHTTP client when requested. No decryption, re-encryption or server is used.

## 4. How it works internally

- Scan \`capes/\` once on game startup, sorted by filename.
- Validate ZIP/PNG/optional native Persona manifest with bounded memory,
  reject duplicate IDs and oversized/invalid packs.
- Auto-create a local wardrobe item and PlayFab \`GetPublishedItem\` metadata
  in RAM using deterministic UUIDs.
- Append these generated items to the existing \`DressingRoom_Capes\` page,
  preserving official entries and original account inventory.
- Serve local asset requests at \`https://cosmetics.invalid/personabinary/UUID.zip\`
  and optional \`https://cosmetics.invalid/thumbnails/UUID.png\` entirely inside
  WinHTTP, with no DNS/TLS request to Cosmos or anywhere else for those assets.
- ZIP contents are never extracted to the game's filesystem. Files in
  \`overrides/\` are not needed for user-created local capes.

## 5. Plaintext and actual equipment

A pack containing only \`cape.png\` creates a plaintext *experimental*
Persona-shaped ZIP containing \`manifest.json\`, \`contents.json\`, and
\`textures/cape.png\`, with no encryption keys.

Minecraft's ordinary resource/skin packs can be plaintext, but a Persona
cape is **not** equivalent to a resource pack. The game's marketplace Persona
parser, account checks, or graphics pipeline may reject this generated ZIP.
**Displaying a locally injected cape is not evidence that it can be equipped
or rendered by other players.** Additional game-native format validation is
required before claiming actual equip compatibility.

An explicit original \`persona.zip\` is more suitable for verifying the
download/replay chain, but valid signatures and authorized use still matter.

## 6. Diagnostics

\`logs/cosmetics.log\` includes:

\`\`\`text
[Cosmetics] local_capes folder=capes loaded=2
[Cosmetics] local_capes: loaded local cape pack My First Cape.zip: My First Cape
[Cosmetics] local cape virtual resource delivered kind=PersonaZip bytes=...
[Cosmetics] local cape virtual resource delivered kind=Png bytes=...
\`\`\`

If the first line shows 0, check ZIP entries are at root level and the file
extension is ZIP. In the Minecraft HTTP trace, \`action=replace_json\` means
the generated items were inserted; \`client_read_complete\` shows the modified
HTTP response was consumed, not that the cape was equipped.


## ZIP 统计与市场回归验证

v0.1.8 会记录：

```text
[Cosmetics] local_capes ZIP_SCAN complete discovered=2 validated=1 rejected=1 skipped=0 registered_items=1 registered_assets=2 native_persona=0 experimental_plaintext=1 in_game_loaded=UNVERIFIED
[Cosmetics] local_capes: ZIP_VALIDATED working.zip: working payload=experimental_plaintext_png
[Cosmetics] local_capes: ZIP_REJECTED broken.zip: invalid outer ZIP
```

以上仅表示本地资源在内存中注册成功，并不表示游戏市场/更衣室接受它。
如果市场返回 401，先用根节点 `enabled=false` 禁用全部 Hook 做 A/B
再继续验证本地披风，避免将鉴权失败归因于图片解析。
