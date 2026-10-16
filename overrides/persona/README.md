# Persona 资源测试

默认提供 \`30074de9-0c60-4096-b051-758c2d2f9488.zip\`。

**这是纯 manifest 的网络响应诊断包，不含真正披风图片和 Persona 二进制，
不符合实际可装备披风资源要求。** 用于确认本地虚拟 ZIP 是否被请求、
是否返回字节，不能用于验证真实披风的可预览或装备能力。

该包只包含明文 \`manifest.json\`，Cosmetics 不读取披风图片，不解密，
也不会覆盖任何 \`premium_cache/persona\` 的原版文件。

真实自定义披风应替换为自己有权使用的有效 Persona 包，保持同名
UUID 和 \`Catalog/GetPublishedItem\` 响应的 \`Contents.Id\` / URL 一致。
游戏仍可能执行签名、完整性及权益检查。

如不希望更衣室展示诊断条目，将 \`overrides/capes_items.json\` 改为 \`[]\`
并重启游戏。原版用户披风权益始终不能被覆盖。
