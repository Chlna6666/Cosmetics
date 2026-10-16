# Cosmetics 0.1.1：原版更衣室联调说明

## 两种“拦截成功”并不相同

1. DLL 启动：出现 \`WinHTTP hooks installed\` 表示 Hook 安装完成。
2. HTTP 请求命中：\`request #N: route matched dressing_room_capes (enabled)\`。
3. 服务端响应收到：\`request #N: WinHttpReceiveResponse ok\` 或
   \`request #N: response headers callback\`。
4. 响应被修改：\`dressing_room_capes: synchronous JSON overlay applied\`
   或 \`dressing_room_capes: async response overlay applied\`。
5. 若出现 \`no local additions\`，说明命中但当前文件没有新增条目。
6. 若出现 \`overlay failed\`，表示 JSON 不匹配或资源异常，保留原始响应。

\`request #N\` 是本进程局部的单调递增编号，不是玩家 ID。

## 预置测试披风

内置测试披风名：\`Cosmetics HTTP Diagnostic Cape\`。
\`overrides/capes_items.json\` 已含 1 个 NotOwned 测试条目。
\`overrides/catalog/18f19bc4-cc0b-4d73-bc4a-894ee3fb5bbd.json\`
提供单件商品元数据。\`overrides/persona/30074de9-0c60-4096-b051-758c2d2f9488.zip\`
是仅含 manifest.json 的 **非可装备** ZIP 测试包。

它不修改正版库存，不持有真实 Persona 图片，不能用于穿戴。
如果更衣室出现测试披风名字，说明目录响应替换成功；
如果同时出现本地 ZIP 的日志，则说明游戏下载链路尝试获取资源。
没有真实可用的 Persona 资源，不能据此判断披风可预览、可穿戴。

删除测试项的方法：把 \`overrides/capes_items.json\` 修改为 \`[]\` 并重启游戏。
此功能不会覆盖任何官方 premium_cache/persona 内容。

## 检查日志

日志默认位置：Cosmetics.dll 同目录下的 \`logs/cosmetics.log\`。
启动 Minecraft → 进入原版更衣室 → 打开披风选项 → 返回再打开。

正常诊断日志类似：

\`\`\`text
[Cosmetics] Cosmetics v0.1.1-diagnostic; independent WinHTTP module; fail-open enabled
[Cosmetics] WinHTTP hooks installed; open the Dressing Room capes page to see a route match
[Cosmetics] request #12: route matched dressing_room_capes (enabled); async=true
[Cosmetics] request #12: response headers callback dressing_room_capes (enabled); async=true
[Cosmetics] dressing_room_capes: async response overlay applied
\`\`\`

不同版本可能使用同步 WinHTTP，日志会显示 \`async=false\`。

\`catalog request missing itemid\` 在旧版本可能来自
\`WinHttpWriteData\` 返回 \`ERROR_IO_PENDING\` 后原程序未记录请求体。
v0.1.1 会捕获排队的异步请求体，并输出
\`catalog lookup ... body_state=empty_body | invalid_json | missing_itemid | valid_itemid\`。
不会输出 JSON 内容、账号、令牌或收据。

- 只有 \`hooks installed\`：DLL 已加载，但目标路径未命中，
  可能是游戏更早已完成请求、使用其他 HTTP 库，或 URL/请求方法不同。
- \`commerce host observed but path/verb not matched\`：确实经过 WinHTTP，
  但当前版本请求路径/方法与规则不同，需要比较实际的 API 形态。
- \`route matched\` 且 \`overlay failed\`：请求路径正确，响应结构可能变化。
- \`catalog lookup ... body_state=valid_itemid\` 但
  \`catalog item not registered locally\`：这是原版正常商品查询，不应替换。
- \`Persona ZIP found\`：本地 ZIP 已被读取并形成虚拟下载响应，不意味着
  Minecraft 接受了它或可装备。

如果 BMCBL 写出 \`unknown version\`，它不等于 DLL 注入失败。
可查看 BMCBL 是否实际读取了随 ZIP 打包的 \`manifest.json\`
（\`version\` 字段为 \`0.1.1\`），并以 Cosmetics 日志为注入证据。

## 安全边界

默认 \`official_inventory_preserving_append.enabled=false\`。原版
库存、购买记录、收据不能被直接替换。测试资源均标记 \`NotOwned\`。
绝不上传玩家原始网络请求、XBL 令牌、完整 inventory 数据、收据等隐私信息。

CI 通过表示 Rust 测试和 Windows DLL 编译成功，不代表游戏内异步回调、
压缩头解析、官方缓存和 Persona 权益校验通过。

## v0.1.3 debug output

- `version=0.1.3`: actual Cargo-embedded DLL version. The old `v0.1.1-diagnostic` was a hardcoded label and did not prove which DLL was injected.
- `unmatched commerce request GET store/PersonaCreatorPage_<redacted>`: WinHTTP was used but configured route did not match. Actual query strings/IDs are not logged.
- `catalog lookup ... body_shape=top_itemid|top_id|nested_id|unknown_fields`: detects PlayFab field-name variants without writing the values or auth tokens.
- `headers ... status=Some(200) ... encoding=br`: a target response was received. It may remain unmodified.
- `action=replace_json ... replacement_bytes=N`: overlay engine created a replacement entity.
- `action=client_read_complete ... delivered_bytes=N`: Minecraft's WinHTTP client consumed the generated entity.
- `action=passthrough reason=overlay_error`: original data retained.
- `persona_profile`: a Cosmos-style navigation tile into DressingRoom_Capes, only when the original StoreRow and dropdownId 0 are present. On unknown page structure, leaves the original response unchanged.

If the packaged `cosmetics.json` was not updated, older local rules take precedence and the new profile overlay remains disabled. Replace config along with DLL for this test.

## v0.1.4 开发调试日志

默认配置：

```json
"debug": {"verbose": true, "response_shape": true}
```

安装更新后完全关闭并重新打开游戏，确认 `session_start build_version=0.1.4`。
`logs/cosmetics.log` 为追加日志，每次启动以分隔符区分。
也可用 DebugView 或 Visual Studio 调试输出查看 `[Cosmetics DEBUG]`。

- `REQUEST_CAPTURED`：捕获请求体，标记捕获长度和完整性。
- `SEND_RESULT`：WinHTTP 已接受、进入异步队列或发送失败。
- `RESPONSE_HEADERS_CAPTURED`：得到响应状态码、长度与编码类型。
- `RESPONSE_CAPTURED`：已读取响应体及完整/不完整状态。
- `UPSTREAM_JSON`：原始数据的非敏感布局/列表数量摘要。
- `RESPONSE_REPLACED`：在内存中生成了替换的响应字节。
- `MODIFIED_JSON`：修改后 JSON 列表数量摘要，可与原版对比。
- `CLIENT_CONSUMED_OVERLAY`：游戏 WinHTTP 消费者已读完替换字节。
- `HANDLE_CLOSE`：关闭时记录缓存与消费字节数。

只有 `route matched` 而没有 `RESPONSE_CAPTURED` 表示尚未证实收到完整响应体。
有 `RESPONSE_REPLACED` 而没有 `CLIENT_CONSUMED_OVERLAY` 说明未观察到消费完毕。
调试版本仍不记录 Authorization、XBL 凭据、原始账户库存、完整响应 JSON。


## v0.1.5 接口匹配已修复

实机日志出现 `POST store/DressingRoom_Capes` 与
`POST store/DressingRoom_PersonaProfile`，证实旧版 GET 匹配是漏拦截原因。
默认商城规则现在支持 `GET,POST` 两种方法。
成功命中后应依次观察 `route matched`、`RESPONSE_HEADERS_CAPTURED`、
`RESPONSE_CAPTURED`、`RESPONSE_REPLACED`、`CLIENT_CONSUMED_OVERLAY`。
仍有异常时根据新日志排查 JSON schema，而不是继续将问题归因于未安装 Hook。
## v0.1.6：官方披风 UI 状态与库存探针

在 `--official-preview` 生成白名单以后，更衣室 UI 修改前后的
`UPSTREAM_JSON`、`MODIFIED_JSON` 日志会包含
`purchased_minecraft_capes_ui=N` 计数差异。
计数代表被 UI 标记为 Purchased 的 Minecraft 披风数量，
**不能解释为真实账号拥有或游戏装备成功**。

只读库存探针（默认关闭）：
`official_inventory_schema_probe`。启用后捕获并输出脱敏布局摘要，
完整原始库存数据、Authorization、收据不落盘、不上传第三方 API。
当前 Cosmos 的远端 `/api/v1.0/player/inventory` 修改算法不在公开仓库中。

## v0.1.8 市场异常隔离与本地披风统计

如果日志中 PlayFab `Catalog/Search` 返回 `401`，代表服务端拒绝该请求。
旧版虽然规则为 disabled，却仍在 session/connection 级别覆盖回调，
可能干扰原版网络处理。新版只给具体启用的 request handle 安装桥接。
仍然需要实机 A/B 验证，无法仅从 401 日志证明具体原因。

快速 A/B 实验：在 cosmetics.json 根节点将 ``"enabled": false`，
彻底关闭游戏并重启。预期 `SAFE_MODE` 及 `WinHTTP hooks NOT installed`；
此状态下市场请求不会被 Cosmetics Hook。如仍有 401，排查账号授权。

`ZIP_SCAN complete discovered=N validated=N rejected=N skipped=N`
只表示本地 ZIP 校验。`registered_items` 表示内存中商品条目数量，
`registered_assets` 是虚拟 Persona/PNG 资源数量。
`native_persona` 和 `experimental_plaintext` 分别区分原生 Persona 包与
实验性 PNG 转换包；`in_game_loaded=UNVERIFIED` 不是错误而是未证实。
出现 `RESPONSE_REPLACED` 只是 HTTP 替换成功，真实装备仍需检查。


## v0.1.9 A/B 检查（无需手动编辑八条规则）

构建包附带 `cosmetics.safe-mode.json` 与 `cosmetics.local-only.json`。
建议先将安全模式文件复制/重命名为 `cosmetics.json`，重启游戏验证
原版市场能否正常显示；再替换为 `cosmetics.local-only.json` 测试。
第二套配置只启用更衣室和本地商品元数据两条规则，
并通过 `local_only=true` 禁止加载旧 Cosmos JSON 和内置示例。
如果仍然只看到 `Catalog/Search` 而没有 `DressingRoom_Capes`，
这是游戏没有调用目标页面，并不表示 ZIP 校验失败。
本地 ZIP 已注册 ≠ 游戏更衣室已接收 ≠ Persona 装备成功。


## v0.1.10 装备链路诊断

`LOCAL_ASSET_REQUEST type=persona_zip`：游戏通过 WinHTTP 请求本地资源 URL。
`LOCAL_ASSET_RESPONSE_READY status=200`：Cosmetics 已准备对应的 ZIP 字节。
`CATALOG_RESPONSE_READY source=local_zip`：本地商品元数据匹配并准备替换。
`source=legacy_or_cosmos`：是旧目录商品或内置 Cosmos 映射。
以上均不能当作真正 `equipped=true`。

如点击后没有 `CATALOG_RESPONSE_READY` 或 `LOCAL_ASSET_REQUEST`，
请保留从点击到失败后的完整日志，后续需要查找非 WinHTTP Persona
装备过程及相关状态，不应伪造 server inventory 的 200/Owned 响应。

## v0.1.11 页面分类

官方 `Official Capes - Catalog Only`、第三方 `Cosmos & Community Capes`、个人 `My Local ZIP Capes` 是同一 `DressingRoom_Capes` 响应里的不同 `GridList`，按实际数据动态出现。每个追加区域复用当前服务端页面中的 GridList/Preview 组件，原始 `Owned` 与其他原版栏目保留。若无法找到正确 GridList 模板，原响应会按 fail-open 原样保留。分类不改变账户权益。

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
