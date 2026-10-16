# Diagnostic sample; opt-in only

Use these two JSON files as **templates**, not as verified Minecraft-compatible
cosmetic items. They do not provide server-side ownership or valid signatures.

- Add the **object** from \`cape_item.json\` to the JSON array in
  \`overrides/capes_items.json\`.
- Save \`catalog.json\` to
  \`overrides/catalog/18f19bc4-cc0b-4d73-bc4a-894ee3fb5bbd.json\`.
- Place a user-owned Persona ZIP as
  \`overrides/persona/30074de9-0c60-4096-b051-758c2d2f9488.zip\`.
  The ZIP root must contain manifest.json and a persona_piece module.
- Verify DLL logs, then inspect whether Minecraft requests the virtual ZIP.
- Do not alter Microsoft entitlement receipts or existing premium_cache/persona.

A generic unsigned ZIP may download successfully and still be rejected on
preview/equip. This is expected until the format acceptance test passes.
