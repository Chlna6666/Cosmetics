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
