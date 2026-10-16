#!/usr/bin/env python3
"""Package multiple author-created capes as one deterministic Cosmetics ZIP.

Persistent pack.json is written ONCE in the source directory, including all UUIDs.
Subsequent builds reuse the same ids. The game never calls uuid4 on startup.
"""
import argparse
import json
from pathlib import Path
import re
import struct
import uuid
import zipfile

PNG = b"\x89PNG\r\n\x1a\n"


def check_png(data):
    if len(data) < 33 or data[:8] != PNG or data[12:16] != b"IHDR":
        raise ValueError("not a PNG image")
    width, height = struct.unpack(">II", data[16:24])
    if not 0 < width <= 2048 or not 0 < height <= 2048:
        raise ValueError("PNG dimensions must be within 1..2048")
    if len(data) > 8 * 1024 * 1024:
        raise ValueError("PNG image must not exceed 8 MiB")


def validate_version(value):
    parts = value.split(".")
    if len(parts) != 3 or any(not part.isdigit() or int(part) > 65535 for part in parts):
        raise ValueError("version must be major.minor.patch, each part <= 65535")
    return value


def safe_key(value):
    return bool(re.fullmatch(r"[a-zA-Z0-9_.-]{1,64}", value)) and value not in (".", "..")


def make_key(stem, used):
    base = re.sub(r"[^a-zA-Z0-9_.-]", "_", stem).strip("._")
    if not base:
        base = "cape"
    base = base[:54]
    key = base
    n = 2
    while key in used:
        key = f"{base}_{n}"
        n += 1
    return key


def create(source, destination=None, name=None, author=None, version=None):
    source = Path(source).resolve()
    if not source.is_dir():
        raise ValueError("source must be a folder of PNG cape images")
    textures = sorted(source.glob("*.png"), key=lambda p: p.name.casefold())
    natives = sorted(source.glob("*.persona.zip"), key=lambda p: p.name.casefold())
    native_stems = {p.name[:-len(".persona.zip")]: p for p in natives}
    stems = sorted({p.stem for p in textures}.union(native_stems),
                   key=str.casefold)
    if not stems or len(stems) > 64:
        raise ValueError("cape folder must contain 1..64 PNG or .persona.zip entries")
    by_stem = {p.stem: p for p in textures}
    destination = Path(destination) if destination else Path("capes") / f"{source.name}.zip"
    dest_resolved = destination.resolve()
    if dest_resolved == (source / "pack.json"):
        raise ValueError("cannot overwrite source pack.json")
    manifest_file = source / "pack.json"
    if manifest_file.exists():
        manifest = json.loads(manifest_file.read_text(encoding="utf-8"))
        if manifest.get("format_version") != 1 or not manifest.get("id"):
            raise ValueError("existing pack.json is not Cosmetics pack format v1")
        uuid.UUID(manifest["id"])
    else:
        manifest = {
            "format_version": 1,
            "id": str(uuid.uuid4()),  # created ONCE, persisted to pack.json
            "name": name or source.name,
            "author": author or "Player",
            "version": "1.0.0",
            "capes": []
        }
    if name:
        manifest["name"] = name
    if author:
        manifest["author"] = author
    if version:
        manifest["version"] = validate_version(version)
    validate_version(manifest.get("version", "1.0.0"))
    previous = {
        (entry.get("texture") or entry.get("persona_zip")): entry
        for entry in manifest.get("capes", [])
    }
    cape_entries = []
    paths = []
    used = set()
    for stem in stems:
        png = by_stem.get(stem)
        native = native_stems.get(stem)
        if png:
            data = png.read_bytes()
            check_png(data)
        else:
            data = None
        original_zip = native.read_bytes() if native else None
        if original_zip is not None:
            if len(original_zip) > 64*1024*1024:
                raise ValueError("Persona ZIP exceeds 64 MiB")
            with zipfile.ZipFile(native) as z:
                payload = json.loads(z.read("manifest.json"))
                if not any(x.get("type") == "persona_piece"
                           for x in payload.get("modules", [])):
                    raise ValueError(f"no persona_piece module in {native}")
        path_png = f"textures/{stem}.png" if png else None
        path_native = f"native/{stem}.zip" if native else None
        key = path_png or path_native
        last = previous.get(key) or {}
        ident = last.get("id")
        if not ident or not safe_key(ident) or ident in used:
            ident = make_key(stem, used)
        used.add(ident)
        item_uuid = last.get("uuid") or str(uuid.uuid4())
        uuid.UUID(item_uuid)
        item = {"id": ident, "uuid": item_uuid,
                "name": last.get("name") or stem}
        if path_png:
            item["texture"] = path_png
            paths.append((path_png, data))
        if path_native:
            item["persona_zip"] = path_native
            paths.append((path_native, original_zip))
        if last.get("author"):
            item["author"] = last["author"]
        cape_entries.append(item)
    manifest["capes"] = cape_entries
    # Limit upload size before generating anything. ZIP compressed size is also
    # checked by the runtime.
    if sum(len(data) for _, data in paths) > 128*1024*1024:
        raise ValueError("uncompressed cape payload total exceeds 128 MiB")
    dest_resolved.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(dest_resolved, "w", zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("pack.json", json.dumps(manifest, ensure_ascii=False, indent=2))
        for path, data in paths:
            archive.writestr(path, data)
    # Persist only AFTER a valid ZIP is successfully written.
    manifest_file.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n",
                             encoding="utf-8")
    print(f"Created {dest_resolved}, capes={len(cape_entries)} pack_id={manifest['id']}"
          f" version={manifest['version']}")
    print(f"Persistent IDs saved at: {manifest_file}")
    print("On texture changes, increment --version 1.0.1 to refresh content URLs/cache.")
    print("Plain PNG -> Persona format remains experimental and may not equip.")
    return dest_resolved


def main():
    parser = argparse.ArgumentParser(
        description="Bundle many capes into one ZIP, with permanent UUIDs")
    parser.add_argument("folder", help="Folder with *.png and optional *.persona.zip")
    parser.add_argument("--out", help="Destination ZIP; default capes/<foldername>.zip")
    parser.add_argument("--name", help="Pack title")
    parser.add_argument("--author", help="Pack author")
    parser.add_argument("--version", help="Explicit version, bump when textures change")
    args = parser.parse_args()
    try:
        create(args.folder, args.out, args.name, args.author, args.version)
    except (ValueError, OSError, KeyError, zipfile.BadZipFile) as exc:
        parser.error(str(exc))


if __name__ == "__main__":
    main()
