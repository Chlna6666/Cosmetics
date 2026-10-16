# Local capes

Put **one ZIP per cape** here. That's the entire installation procedure.
Cosmetics scans this directory when the DLL starts. Restart Minecraft
after adding, editing, or removing a file.

Minimal ZIP:

    my_cape.zip
      cape.png

Optional metadata inside the same ZIP:

    my_cape.zip
      cape.png
      cape.json       {"name":"My Cape","author":"Player"}

Optional native Persona payload:

    my_cape.zip
      cape.png        (thumbnail, optional if persona.zip exists)
      cape.json       (optional)
      persona.zip     (original native Persona archive, unchanged)

If cape.json is absent, name defaults to ZIP filename, author to "Player",
and stable local UUIDs are automatically derived from filename, name and
author. A fixed "id" UUID in cape.json avoids ID changes when you rename
the pack. You do not need a catalog/ folder or any route configuration.

Try:

    py scripts/pack_cape.py your-cape.png --name "My Cape"

PNG-only packs use an experimental, plaintext Persona resource format.
**They may appear in the menu without actually being wearable.**
A prebuilt persona.zip is forwarded without re-encryption or modification,
but still subject to Minecraft validation. No Cosmos access is required.
