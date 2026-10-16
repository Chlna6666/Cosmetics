# Persona resources

The default cape list contains Android Cape and Minecon 2011 Cape metadata.
Their PlayFab catalog JSON files point to Cosmos' original HTTPS CDN. The
actual ZIP bytes are **not embedded in this repository or Windows installer**.

The local `30074de9-0c60-4096-b051-758c2d2f9488.zip` is a
**manifest-only diagnostic fixture**, with no actual cape image. It is not
an equipable Persona cape, and it is not referenced by the default cape list.

Use `scripts/import_cosmos.py` to populate `overrides/catalog/` and
`overrides/capes_items.json` with additional metadata. For a local ZIP
download, `--bundle-zips --confirm-asset-rights` performs a voluntary
end-user-local download from the upstream CDN and changes the local catalog
URL to the DLL's virtual HTTPS host. It doesn't manufacture Persona signatures
or grant server-side ownership. Never overwrite real premium_cache/persona.
