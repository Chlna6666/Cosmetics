# Cosmos → Cosmetics resource integration

## What is packaged

Cosmetics v0.1.2 ships with *locally authored metadata templates* for
**Android Cape** and **Minecon 2011 Cape**, based on the item IDs and asset URLs
published in Bedrock-Cosmos/Responses. This permits a real PlayFab
\`GetPublishedItem\` response to point at the publisher's existing HTTPS
\`personabinary\` ZIP resource, without a proxy or a local web server.

The two minimal JSON metadata templates are also compiled into Cosmetics.dll
as built-in fallbacks. A local file of the same name always has priority.
The actual third-party cape art, signatures, asset ZIPs, full storefront
pages and other content are not bundled. Neither upstream repository has an
obvious repository-wide license granting full republication rights for all
third-party cape assets. Public GitHub visibility does not itself authorize
redistribution.

## Install and test

Extract the GitHub Actions artifact. Keep the following paths beside the DLL:

\`\`\`text
Cosmetics.dll
cosmetics.json
overrides/
  capes_items.json
  catalog/
    5bba5540-377b-4ec2-9633-4551c4e82e37.json
    e847fc66-b988-4065-bdc9-fa8fd17457b2.json
  persona/
    README.md
scripts/
  import_cosmos.py
\`\`\`

Open the original Dressing Room. The two preset cape entries have
\`ownership: "NotOwned"\`: a client-rendered entry is **not** proof of a
legitimate entitlement or support for equipment / multiplayer visibility.
\`GetPublishedItem\` must match the same item ID and subsequent downloads
must use either the original Cosmos CDN URL or an opt-in locally stored ZIP.

Cosmetics appends to the official page, preserving every existing official
item. The \`itemListComp.totalItems\` and
\`customStoreRowConfiguration.maxOffers\` fields are updated together, using
the actual resultant array length. The official inventory interception stays
disabled by default.

## Add more publicly listed cape metadata

Run from the extracted Cosmetics install directory (requires Python 3):

\`\`\`powershell
py scripts/import_cosmos.py --destination . --limit 15
py scripts/import_cosmos.py --destination . --limit 30 --name cape
\`\`\`

The importer queries \`Bedrock-Cosmos/Responses\` at its current GitHub
\`main\` branch, reads \`MainPages/Capes.json\` and
\`LauncherJsons/PlayfabGetPublishItemResponses.json\`, follows only registered
Persona catalog mappings, and writes:
- \`overrides/capes_items.json\` — deduplicated list, imported ownership always
  \`NotOwned\`; existing local entries are retained.
- \`overrides/catalog/<item-uuid>.json\` — matching PlayFab catalog metadata,
  validated to use \`personabinary\` and the pinned Cosmos ZIP CDN domain.

This is a local opt-in fetch initiated by the user; the release build itself
does not mirror upstream source files or ZIP archives.

## Optional local ZIP mirror (personal use only)

Only when you have the rights to use and store the selected assets:

\`\`\`powershell
py scripts/import_cosmos.py --destination . --limit 3 --bundle-zips --confirm-asset-rights
\`\`\`

ZIPs are downloaded to \`overrides/persona/<Contents.Id>.zip\` and matching
local catalog URLs are rewritten to
\`https://cosmetics.invalid/personabinary/<Contents.Id>.zip\`.
The URL is handled entirely in the injected WinHTTP layer. It is not
sent to an external server, and the DLL does not bypass signatures, modify
a Microsoft account or write \`premium_cache/persona\`.

**Important limitations:** The ZIP loader inspects a plaintext
\`manifest.json\` containing a \`persona_piece\` module. A real resource may
be packaged differently. Some Cosmos metadata uses different
\`Contents.Id\` and \`DisplayProperties.packIdentity[0].uuid\`; do not
assume they are interchangeable. The original game's signature /
integrity checks may still reject the package.

## Sources and attribution

- https://github.com/BE-Community-Dev/Bedrock.Cosmos/
- https://github.com/Bedrock-Cosmos/Responses/
- https://github.com/Bedrock-Cosmos/Responses/blob/main/MainPages/Capes.json

These URLs identify reference material; inclusion does not imply project
affiliation, sponsorship or redistribution permission.


## v0.1.5：完整导入

上游 `Capes.json` 当前列出 270 个披风商品，其中 245 个在
`PlayfabGetPublishItemResponses.json` 可找到对应的本地响应映射。
正常运行可以直接使用：

```powershell
py scripts/import_cosmos.py --destination . --all
```

`--limit 0` 等价 `--all`，现在默认不再限制前 15 个。
如果需要包括没有本地 PlayFab 元数据的 25 项，可以显式追加
`--include-unmapped`，但对应项可能无法预览或下载。
为减轻一次请求全部元数据的耗时，脚本使用可调并行数
`--workers 8`（范围 1–16），并输出每 25 项的进度。
脚本不会改变官方账号权益。

**游戏 26.40 的实际页面方法是 POST，不是 GET。**
如果仍使用旧 cosmetics.json，就算更新 DLL 也会漏掉
`DressingRoom_Capes` 与 `DressingRoom_PersonaProfile`。