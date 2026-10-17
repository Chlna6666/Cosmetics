#!/usr/bin/env python3
"""Bundle up to 64 local capes and persist UUIDs safely in pack.json (v1)."""
import argparse
import json
import os
from pathlib import Path
import re
import sys
import tempfile
import uuid
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parent))
from cape_pack_utils import (check_png, texture_info, valid_name, validated_rarity,
                             validated_start_date, validate_native)


def validate_version(value):
    parts = value.split(".")
    if len(parts) != 3 or any(not p.isascii() or not p.isdigit() or len(p) > 5 or int(p) > 65535 for p in parts):
        raise ValueError("version must be major.minor.patch, each part <= 65535")
    return value


def safe_key(value):
    return bool(re.fullmatch(r"[a-zA-Z0-9_.-]{1,64}", value)) and value not in (".", "..")


def make_key(stem, used):
    base = re.sub(r"[^a-zA-Z0-9_.-]", "_", stem).strip("._")[:54] or "cape"
    key, n = base, 2
    while key in used:
        key, n = f"{base}_{n}", n + 1
    return key


def create(source, destination=None, name=None, author=None, version=None, rarity=None):
    source = Path(source).resolve()
    if not source.is_dir():
        raise ValueError("source must be a folder of PNG cape images")
    textures = sorted(source.glob("*.png"), key=lambda p: p.name.casefold())
    natives = sorted(source.glob("*.persona.zip"), key=lambda p: p.name.casefold())
    by_stem = {p.stem: p for p in textures}
    native_stems = {p.name[:-len(".persona.zip")]: p for p in natives}
    stems = sorted(set(by_stem).union(native_stems), key=str.casefold)
    if not 1 <= len(stems) <= 64:
        raise ValueError("cape folder must contain 1..64 PNG or .persona.zip entries")
    if len({stem.casefold() for stem in stems}) != len(stems):
        raise ValueError("cape filenames must be unique ignoring case")
    for stem in stems:
        if (not stem or len(stem.encode("utf-8")) > 140
            or any(c in stem for c in ("/", "\\", ":")) or stem in (".", "..")):
            raise ValueError(f"unsafe cape filename: {stem}")
    destination = Path(destination) if destination else Path("capes") / f"{source.name}.zip"
    dest_resolved = destination.resolve()
    manifest_file = source / "pack.json"
    if dest_resolved == manifest_file:
        raise ValueError("cannot overwrite source pack.json")
    if manifest_file.exists():
        manifest = json.loads(manifest_file.read_text(encoding="utf-8"))
        if manifest.get("format_version") != 1:
            raise ValueError("existing pack.json is not Cosmetics pack format v1")
        uuid.UUID(manifest["id"])
    else:
        manifest = {"format_version": 1, "id": str(uuid.uuid4()),
                    "name": source.name, "author": "Player", "version": "1.0.0", "capes": []}
    manifest["name"] = valid_name(name or manifest["name"], "pack name")
    manifest["author"] = valid_name(author or manifest["author"], "pack author")
    manifest["version"] = validate_version(version or manifest.get("version", "1.0.0"))
    if rarity is not None:
        validated_rarity(rarity)
    previous = {}
    for entry in manifest.get("capes", []):
        for path in (entry.get("texture"), entry.get("persona_zip")):
            if path:
                previous[path] = entry
    capes, paths, used = [], [], set()
    for stem in stems:
        png, native = by_stem.get(stem), native_stems.get(stem)
        img = png.read_bytes() if png else None
        dimensions = check_png(img) if img is not None else None
        original = validate_native(native) if native else None
        path_png = f"textures/{stem}.png" if png else None
        path_native = f"native/{stem}.zip" if native else None
        last = previous.get(path_png) or previous.get(path_native) or {}
        ident = last.get("id")
        if not ident or not safe_key(ident) or ident in used:
            ident = make_key(stem, used)
        used.add(ident)
        item_id = str(uuid.UUID(last["uuid"])) if last.get("uuid") else str(uuid.uuid4())
        item = {"id": ident, "uuid": item_id,
                "name": valid_name(last.get("name") or stem, "cape name")}
        if path_png:
            item["texture"] = path_png
            item["texture_info"] = texture_info(*dimensions)
            paths.append((path_png, img))
        if path_native:
            item["persona_zip"] = path_native
            paths.append((path_native, original))
        if last.get("author"):
            item["author"] = valid_name(last["author"], "cape author")
        if rarity is not None or last.get("rarity"):
            item["rarity"] = validated_rarity(rarity or last["rarity"])
        if last.get("startDate"):
            item["startDate"] = validated_start_date(last["startDate"])
        capes.append(item)
    manifest["capes"] = capes
    payload = json.dumps(manifest, ensure_ascii=False, indent=2) + "\n"
    if len(payload.encode("utf-8")) > 64 * 1024:
        raise ValueError("pack.json exceeds 64 KiB")
    if sum(len(data) for _, data in paths) > 128 * 1024 * 1024:
        raise ValueError("uncompressed cape payload exceeds 128 MiB")
    dest_resolved.parent.mkdir(parents=True, exist_ok=True)
    # Keep previous output untouched if ZIP generation fails; atomic replacement.
    with tempfile.NamedTemporaryFile(dir=dest_resolved.parent, suffix=".zip", delete=False) as temp:
        temp_path = Path(temp.name)
    try:
        with zipfile.ZipFile(temp_path, "w", zipfile.ZIP_DEFLATED) as archive:
            archive.writestr("pack.json", payload)
            for path, data in paths:
                archive.writestr(path, data)
        if temp_path.stat().st_size > 64 * 1024 * 1024:
            raise ValueError("outer cape ZIP exceeds 64 MiB")
        os.replace(temp_path, dest_resolved)
    finally:
        temp_path.unlink(missing_ok=True)
    # Persist only after a valid package is produced.
    manifest_file.write_text(payload, encoding="utf-8")
    print(f"Created {dest_resolved}, capes={len(capes)} pack_id={manifest['id']} version={manifest['version']}")
    print(f"Persistent UUIDs: {manifest_file}")
    print("Native animated Persona bundles are kept as-is. Plain PNGs become static Persona capes.")
    return dest_resolved


def main():
    parser = argparse.ArgumentParser(description="Bundle PNG / native Persona capes with stable UUIDs")
    parser.add_argument("folder", help="Folder with *.png and optional *.persona.zip")
    parser.add_argument("--out", help="Destination ZIP; default capes/<foldername>.zip")
    parser.add_argument("--name", help="Pack name")
    parser.add_argument("--author", help="Pack author")
    parser.add_argument("--version", help="Version; content URLs also change automatically")
    parser.add_argument("--rarity", choices=["common","uncommon","rare","epic","legendary"])
    args = parser.parse_args()
    try:
        create(args.folder, args.out, args.name, args.author, args.version, args.rarity)
    except (ValueError, OSError, KeyError, zipfile.BadZipFile) as exc:
        parser.error(str(exc))


if __name__ == "__main__":
    main()
