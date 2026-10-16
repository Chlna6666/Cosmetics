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


## v0.1.10 本地 PNG 预览及原生资源 UUID

- `cape.png` 必须是能实际解码的 PNG，旧版只检查 PNG 头导致误报。
- 支持 64×32、128×64 等成比例 UV 图，将披风背面裁切绘制成 64×64 的商品缩略图。
- 原生 `persona.zip` 采用 `manifest.header.uuid` 与 `header.version` 作为商品 packIdentity；
  `modules[].uuid` 是另一种身份，不能替换 header.uuid。
- 由 PNG 生成的实验性 Persona ZIP 现在也保持 header UUID 与 packIdentity 一致。

真正 Bedrock Persona 缓存并不只是明文资源包：可见二进制 `contents.json`、
`signatures.json`，并可能执行额外校验。**当前 PNG-only 格式仍是实验性质，
不能保证游戏内穿戴，更不保证多人可见。**

点击披风后请关注 `CATALOG_RESPONSE_READY`、`LOCAL_ASSET_REQUEST` 与
`LOCAL_ASSET_RESPONSE_READY`，以及对应的客户端读取完成事件。
如果连资源请求都没有，不能把失败归因于 ZIP 内部解密，而应继续追踪商品和装备逻辑。

## v0.1.11 独立本地披风栏目

`capes/*.zip` 仍只需要一个 ZIP。游戏正常请求 `DressingRoom_Capes` 页面后，Cosmetics 会把本地 ZIP 放入 `My Local ZIP Capes` 栏，和 `Official Capes - Catalog Only`、`Cosmos & Community Capes` 分离；原版 Owned 列表不修改。分类不代表真正装备成功。


## v0.1.12 — Multiple capes per ZIP and stable identities

Both formats work in the same `capes/` directory:

- Existing ZIP: `cape.png` and optional `cape.json` / `persona.zip` (one cape).
- Multi-cape ZIP: `pack.json` plus up to 64 declared cape textures/native archives.

To create a package without editing UUIDs manually, place several PNGs inside
a folder (for example `MyCapeCollection/`) and execute:

```powershell
py scripts/pack_capes.py MyCapeCollection --name "My Collection" --author "Player"
```

This writes `capes/MyCapeCollection.zip` and stores permanent UUIDs in
`MyCapeCollection/pack.json`. Keep that source manifest when rebuilding.
Re-running the script reuses all existing IDs; new PNGs get new entries.

```text
MyCapeCollection.zip
├── pack.json
└── textures/
    ├── red.png
    └── blue.png
```

```json
{
  "format_version": 1,
  "id": "11111111-2222-4333-8444-555555555555",
  "name": "My Cape Collection",
  "author": "Player",
  "version": "1.0.0",
  "capes": [
    {"id": "red", "name": "Red Cape", "texture": "textures/red.png"},
    {"id": "blue", "name": "Blue Cape", "texture": "textures/blue.png"}
  ]
}
```

Each cape key `id` is stable inside the pack. The package UUID and key
deterministically produce stable game product UUIDs. You may also supply
a fixed per-cape `uuid` in the manifest. No UUID is randomly generated by
the DLL at startup. Renaming the outer ZIP and reordering the capes do not
change identities; the legacy single-cape UUID derivation remains intact.

**Cache updates**: A regular restart retains the same UUIDs, versions,
and virtual URLs. When textures change, explicitly bump the package
version with `--version 1.0.1` so the virtual resource URL changes.
The product UUID and pack UUID stay stable. This avoids unnecessary
cache misses and refreshes changed assets, but does not override
Minecraft's independent entitlement or marketplace caches.

Each pack is validated and registered atomically, preventing partial
registration when one cape is invalid. Logs report validated ZIP count
separately from registered cape count. The older Cosmos overrides
remain independent. Optional `persona_zip` is passed through unchanged;
plain PNG-generated Persona assets are experimental and may not equip.

## v0.1.13 — 高分辨率和商品死锁

1. 已修复 `catalog_fast_response` 日志在持有 `tables` Mutex 时再次加锁造成的死锁。
2. 已知本地商品 ID 不再发送给 PlayFab；WinHTTP 直接返回虚拟 HTTP 200。
3. `CAPE_REGISTERED` 输出每件披风贴图的分辨率、来源和实验性质。
4. 支持 64x32、128x64、256x128、512x256 等成比例 PNG，并原样存入资源。
5. 128x128 等非标准披风 UV 尺寸会报 `invalid cape UV dimensions`。

诊断点击链路时依次看：
`LOCAL_CATALOG_SYNTHETIC`、`client_read_complete`、
`LOCAL_ASSET_REQUEST`、`LOCAL_ASSET_RESPONSE_READY`、`client_read_complete`。
如果只是服务端 400 且没有 `LOCAL_CATALOG_SYNTHETIC`，说明该请求不属于
当前 DLL 已注册的本地商品 ID（或请求体没有完整捕获）。
HTTP 资源送达仍不代表 Minecraft Persona 验证或装备成功。

## v0.1.14 启动与披风全链路日志

Cosmetics 启动时输出版本、作者 Chlna6666、GPL-3.0-only、
`https://github.com/Chlna6666/Cosmetics` 和 Issues 地址。
文件日志：`logs/cosmetics.log`。全部日志也输出 Windows 调试器
`OutputDebugStringW`，可用 DebugView 查看。有现成的 Windows 控制台时
同时输出；GUI Minecraft 默认没有控制台，也不会自动转到 BMCBL 日志。

`ITEM_REGISTERED` 输出条目作者/名称/UUID/Persona pack UUID。
`CAPE_REGISTERED` 表示内存加载贴图并生成实验性资源，
`game_loaded=UNVERIFIED` 代表未证实游戏解析成功。
`LOCAL_CATALOG_SYNTHETIC` 表示本地商品的 HTTP 元数据由 DLL 合成。
`LOCAL_ASSET_REQUEST` 附上本地 `cosmetics.invalid` 的虚拟资源 URL。
`LOCAL_ASSET_RESPONSE_READY` 表示资源字节准备就绪。
`HANDLE_CLOSE` 输出总字节数、已读取字节数和 fully_consumed。

修复了合成 WinHTTP 响应的自定义 Content-Type/Content-Length 查询、
HTTP 状态文本和 HTTP 版本查询。之前这些查询可能访问没有任何网络
响应的原始句柄，导致错误或等待。注意即便 WinHTTP 下载完整，
PNG-only 生成的明文 Persona ZIP 仍未被证实可在游戏里装备。


### Native Persona 与明文试验包的边界（26.40）

Cosmetics 读取 `capes/*.zip` 时，外层 ZIP 只需包含 `cape.png` 和可选 `cape.json`；这些文件不是 Minecraft 可直接加载的 Persona 包。

PNG-only 路径会自动生成带有 `manifest.json`、明文 `contents.json`、`textures/cape.png` 以及语言文件的内部 ZIP。此 ZIP **不是** Marketplace 的 `personabinary` 资源：它缺少真实 Persona 的 `*.meta.json`，也没有 Marketplace 签名。文件齐全并不能证明客户端接受它，尤其不能通过给 PNG 或 ZIP 任意加密来获得有效资源。正式使用应优先提供与相应披风一致、可用的原生 `persona.zip`，通过外层包的 `persona.zip` 字段透传，不对原始二进制进行转换。

当前 WinHTTP 虚拟资源同时支持纯路径与带查询参数的路径；`LOCAL_ASSET_REQUEST`、`LOCAL_ASSET_RESPONSE_READY`、`client_read_complete` 只表示网络拦截阶段，而不代表 Persona 已解析或披风已装备。如果点击时完全没有 `LOCAL_ASSET_REQUEST`，应排查 Minecraft 是否绕过 WinHTTP 使用其他网络栈，不能误报为 ZIP 校验或解密失败。
