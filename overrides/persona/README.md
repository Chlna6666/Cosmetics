# Opaque local Persona packages

Place a Persona ZIP at \`<resource-uuid>.zip\` in this directory. The extension
is \`.zip\`; the name must match the UUID in the \`personabinary\` URL.

Only **manifest.json** is inspected. All binary content and images are carried
as opaque bytes; there is no decryption, signature generation or entitlement
override.

To opt in to offline local ZIP delivery, add a matching catalog response in
\`overrides/catalog/<item-uuid>.json\`, with the payload:

    "Contents": [{
      "Type": "personabinary",
      "Url": "https://cosmetics.invalid/personabinary/<resource-uuid>.zip"
    }]

Cosmetics intercepts the synthetic host *inside the game's WinHTTP client* and
responds using the local ZIP. The host must never be sent to a real DNS resolver.
This works only if the game's resource loader uses the WinHTTP stack. An invalid
or unsigned Persona package may still be rejected by Minecraft. No original
premium_cache/persona files are touched.
