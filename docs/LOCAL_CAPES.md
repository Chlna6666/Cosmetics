# Cosmetics Local Capes — standalone ZIP packs

## 当前版本：高清披风与动画 Persona 打包（2026-10）

Cosmetics 目前在游戏启动时通过 Rust `persona_pack` 从普通 PNG 生成
`personabinary.zip → ppack0.zip → manifest.json / *.meta.json / contents.json / signatures.json`。
**Python 脚本输出的是 Cosmetics 安装用的外层 ZIP**，因此无需在 Python
环境安装加密依赖或手工书写 Persona 的 `*.meta.json`。

### 高清静态披风（支持原像素，不做低清缩放）

```powershell
py scripts/pack_cape.py cape1280x640.png --name "高清披风" --author "作者" --rarity rare --out "capes/高清披风.zip"
```

**Python 不负责高清转换、纹理放大或生成 UV/分辨率 JSON。**
它只是检查文件签名与大小，随后把输入的 PNG 字节原样放进 ZIP。
`cape.json` 仅保存名称、作者等商品信息：

```json
{
  "name": "高清披风",
  "author": "作者",
  "rarity": "rare"
}
```

**Cosmetics DLL 才负责** 解码原图、验证完整 PNG 与 `64×32 × 整数倍率`
的 UV、推导贴图尺寸、生成缩略图，并构造 `*.meta.json`、
加密 Persona 资源索引和最终 `personabinary.zip`。
支持的比例包括 `64×32`、`128×64`、`256×128`、
`512×256`、`1280×640` 和最高 `2048×1024`（PNG 限制 8 MiB）。
如果输入不符合游戏披风格式，Python 可能仍然打包成功，
但 Cosmetics 会在游戏启动时拒绝加载并写出诊断信息。

### 已有动画 Persona ZIP：保持全部内层资源

```powershell
py scripts/pack_cape.py --persona-zip "我的动画.persona.zip" --name "动画披风" --out "capes/动画披风.zip"
```

`--persona-zip` 支持两种来源：

1. 原生裸 Persona ZIP：根目录包含 `manifest.json`、类型为
   `persona_piece` 的模块；Cosmetics 运行时补外层 `ppack0.zip`。
2. 完整的 catalog personabinary ZIP：根目录为 `ppack0.zip`，
   内层包含 Persona manifest；Cosmetics 运行时直接使用完整原始字节。

打包器**不拆帧、不重新加密、不覆盖动画 JSON、贴图、骨骼和其他资源**，
所以原始 Persona 包的动画定义不会被打包过程破坏。
这不保证游戏一定识别该第三方动画内容；只保证无损保留它。

**不要把帧图拼成 64×64、64×96 等竖向图后伪装成静态披风**。
标准 Persona 披风的 `texture_sources` 不能仅依靠这样的图自动播放帧。
CapeForge 等资源包使用的 `.mcpack` 动画控制器是另一条渲染链路；
此 Python 命令不会自动把资源包动画转换为 Persona 商品动画。

### 批量打包

```text
MyCapes/
  red.png
  hires.png
  moving.persona.zip       # 可选：完整原生/封装 Persona
  moving.png               # 可选：仅用于缩略图
```

```powershell
py scripts/pack_capes.py MyCapes --name "我的披风合集" --author "Player" --out "capes/MyCapes.zip"
```

生成的 `MyCapes/pack.json` 采用现有 `format_version: 1`（兼容旧版 Cosmetics），
原始贴图字节不会改变，Python 不写 `texture_info`；Cosmetics 自行识别高清贴图。`uuid` 会保存并在后续重建复用。
不要删除持久化的 `pack.json`。当图片改变时，当前 Rust 版也会根据
最终 Persona 实际内容改变下载 URL，而商品 UUID 保持稳定。

---


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


### Cosmos CustomCapes 商品列表字段

本地披风商品与 `Persona/CustomCapes/*.json` 及 `overrides/capes_items.json` 使用同一套 PersonaDurable 商品字段。自动合成商品信息并通过 `dressing_room_capes` 注入现有 `My Local ZIP Capes` 分类；无需将运行时本地披风再写入固定的 `overrides/capes_items.json`（避免多次启动时出现重复 ID）。

单披风 `cape.json` 或多披风 `pack.json.capes[]` 可以额外声明 `rarity`（common、uncommon、rare、epic、legendary）以及真实的 `startDate`（格式 `YYYY-MM-DDTHH:MM:SSZ`），缺省时不伪造发布日期。商品与 GetPublishedItem 的 `Id`、`packIdentity` 完全一致。对于仅含原生 `persona.zip` 而没有单独贴图的条目，会生成一个可响应的占位缩略图，避免空资源 URL。

Cosmos 的 `linksToInfo`、`namedLinksToInfoMap` 指向专门的 CustomCapes Persona 页面。Cosmetics 默认关闭该页面路由，因此不能简单复制这个链接，否则会造成新的无效导航。当前仍允许在更衣室通过 `personaOfferInteractionComp` 交互。列表元数据完善并不等于游戏已接受或装备实验性 PNG-only Persona ZIP；那还要求内部 Persona 资源被客户端真正解析。

## v0.1.14 Persona binary packaging follow-up (2026-10-17)

The game's first requests for both locally generated Persona ZIPs used **HEAD**, not GET. Older Cosmetics returned HTTP **405**. Virtual HEAD now replies **200** with the same resource type and Content-Length as GET, without a body. Unknown URLs return 404, unrelated methods 405. This fix does not alter marketplace responses or entitlements.

Cosmos and MerelyCapes `personabinary` downloads consist of an outer ZIP containing `ppack0.zip`. The MerelyCapes reference (MIT licensed) generates `*_cape.meta.json` with `piece_id`, `piece_name`, `piece_type=persona_capes`, `zone=[body_back_upper,body_back_lower]` and `texture_sources`. Cosmetics PNG-only imports now use that layout, include the manifest SHA-256 digest in `signatures.json`, and encode resource files and `contents.json` in the same **AES-256/CFB8** format as the public MerelyCapes generator. File keys are reproducibly derived for stable local registration; this is format interoperability, **not Mojang attestation, official inventory, or a guarantee of equip success**. Native, already-bundled Persona assets are passed through byte-for-byte. Bare native Persona packs are wrapped in `ppack0.zip` before being served.

The source and reference have different security contexts: these resources are authored and used locally, not signed by Mojang. Successful tests prove local ZIP/encryption structure but not the game's Persona acceptance. With the new DLL, confirm `LOCAL_ASSET_RESPONSE_READY method=HEAD status=200`, any subsequent GET, complete data delivery, and actual avatar preview. Earlier documentation referring to the unencrypted/metadata-less generator describes superseded builds.

### 资源缓存失效

Persona 下载资源的 UUID 取自稳定商品 UUID、包版本、实际 Persona ZIP 内容和缩略图的 SHA-256 指纹。更换 PNG、Persona ZIP 或打包格式时，`Contents[0].Id` 和下载 URL 自动变化。

**重要（2026-10 高清缓存修复）**：对于 Cosmetics 现场生成的 PNG Persona，除下载 URL 外，现在还会按原始 PNG、披风名称、作者设置的版本及生成器版本推导稳定的三段式 `packIdentity.version`。该版本同时写入 Persona 内部 `manifest.header.version`、模块版本、更衣室商品和 `GetPublishedItem`；同样的输入得到相同版本，不同 PNG 不再一直沿用 `1.0.0`。固定商品 ID、包 UUID 不变。**已有原生 Persona ZIP 不改动版本**，保持原始发布者数据。

日志 `ITEM_REGISTERED` 会输出 `pack_version`，用于核对客户端缓存版本。当前 Minecraft 缓存命中规则不是公开的，以上属于避免资源标识不一致的保守措施；并不证明 Minecraft 一定支持 1280×640 的 Persona 原图。完全退出游戏并重新启动后再复测，**若日志没有任何 `LOCAL_CATALOG_SYNTHETIC`、`LOCAL_ASSET_REQUEST`，则这段日志只能证明商品呈现，不能据此断言游戏收到了新版 ZIP**。

## 2026-10：用户实测高清索引 PNG 与更衣室缩略图修复

根据用户上传的两个完整 ZIP 实测：

- `917813.png.zip` 为 1280×640、8-bit **indexed/palette PNG**（PNG color type 3），Cosmetics 旧版本可以解码并生成缩略图，但直接把原始索引 PNG 嵌入加密 Persona 资源。Minecraft 预览未显示实际穿戴。
- `我的披风1(2).zip` 为 64×32、8-bit **RGBA PNG**（color type 6），用户游戏内穿戴正常。

**修复：** Cosmetics Rust 在生成 **非原生** Persona ZIP 时，检测原始 PNG 编码。对于 indexed、RGB 等非 8-bit RGBA，先由已验证的解码结果重新编码为 **8-bit RGBA PNG**，保持原图分辨率、UV、像素和透明度，不缩放、不转换为 64×32；原本就是 RGBA8 的 PNG 保留原始字节。该规范化只影响游戏内 Persona 资源。预制原生 `persona.zip` 完全保留字节。Python 打包器仍然只包装原始输入，不做任何图片转换。

日志中新增 `source_png_color_type`（3 为索引、6 为 RGBA）及 `persona_rgba_normalized`；用于判断资源链路是否走到转换分支。对于一个 1280×640 palette PNG，期望看到 `source_png_color_type=3 persona_rgba_normalized=true`，然后在按需下载阶段由 WinHTTP 返回已生成的 ZIP。

同时，旧版本地缩略图仅展示一块平面的背面 UV 纹理。新缩略图会将外侧背面 UV（64×32 模板中 `x=12..22, y=1..17`，高清按倍率缩放）渲染到一个中性背部人形示意图上，方便区分披风的实际外侧与内侧。此缩略图是静态离线合成，并不代表游戏内装备状态。

这两项改动分别修复了**上传格式兼容性缺口**与**本地缩略图展示方式**。由于没有来自新 DLL 的游戏端下载、解码及穿戴回调证据，不能只凭单元测试断言 1280×640 的 Persona 在所有 Minecraft 版本中均受支持。若仍不显示，需收集完整点击期间的 `LOCAL_CATALOG_SYNTHETIC`、`LOCAL_ASSET_REQUEST`、`LOCAL_ASSET_RESPONSE_READY` 和 `client_read_complete` 日志。
