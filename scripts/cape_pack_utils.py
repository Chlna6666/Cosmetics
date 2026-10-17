"""Shared, dependency-free validation for Cosmetics authoring ZIPs.

This validates author input; native Persona archives are preserved, never signed
or modified. Rendering of animation inside a Persona bundle is game-dependent.
"""
import json
from pathlib import Path
import re
import uuid
from zipfile import ZipFile, BadZipFile

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
MAX_PNG = 8 * 1024 * 1024
MAX_NATIVE = 64 * 1024 * 1024
MAX_FILES = 1024
MAX_OUTER = 64 * 1024 * 1024
ALLOWED_RARITIES = {"common", "uncommon", "rare", "epic", "legendary"}


def check_png(data):
    """Only verify the file size and PNG signature before archiving.

    The Cosmetics Rust DLL is the sole authority for PNG decoding, UV
    dimensions, high-resolution scale and Persona metadata generation.
    """
    if not isinstance(data, bytes) or not data.startswith(PNG_SIGNATURE):
        raise ValueError("input is not a PNG image")
    if len(data) > MAX_PNG:
        raise ValueError("PNG exceeds 8 MiB")


def valid_name(text, label):
    if not isinstance(text, str) or not text or len(text) > 90 or any(ord(c) < 32 or ord(c) == 127 for c in text):
        raise ValueError(f"invalid {label} (1..90 visible characters)")
    return text


def validated_rarity(value):
    if value not in ALLOWED_RARITIES:
        raise ValueError("rarity must be common, uncommon, rare, epic or legendary")
    return value


def validated_start_date(value):
    if value is not None:
        import datetime
        if not re.fullmatch(r"\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ", value):
            raise ValueError("startDate must be YYYY-MM-DDTHH:MM:SSZ")
        try:
            datetime.datetime.strptime(value, "%Y-%m-%dT%H:%M:%SZ")
        except ValueError as exc:
            raise ValueError("startDate contains invalid calendar values") from exc
    return value


def validate_native(path):
    """Accept a bare Persona zip or a catalog personabinary (ppack0.zip)."""
    path = Path(path)
    if path.stat().st_size > MAX_NATIVE:
        raise ValueError("persona.zip exceeds 64 MiB")
    binary = path.read_bytes()
    try:
        with ZipFile(path) as outer:
            if len(outer.infolist()) > 64:
                raise ValueError("native Persona ZIP has too many entries")
            if "ppack0.zip" in outer.namelist():
                member = outer.getinfo("ppack0.zip")
                if member.file_size > MAX_NATIVE:
                    raise ValueError("ppack0.zip exceeds 64 MiB")
                import io
                with ZipFile(io.BytesIO(outer.read(member))) as inner:
                    check_manifest(inner)
            else:
                check_manifest(outer)
    except (BadZipFile, KeyError) as exc:
        raise ValueError(f"invalid native Persona ZIP: {exc}") from exc
    return binary


def check_manifest(archive):
    if len(archive.infolist()) > MAX_FILES:
        raise ValueError("native Persona pack has too many entries")
    item = archive.getinfo("manifest.json")
    if item.file_size > 64 * 1024:
        raise ValueError("native Persona manifest too large")
    manifest = json.loads(archive.read(item))
    modules = manifest.get("modules", [])
    if not isinstance(modules, list):
        raise ValueError("native Persona modules must be a list")
    module = next((m for m in modules
        if isinstance(m, dict) and m.get("type") == "persona_piece"), None)
    if module is None:
        raise ValueError("native Persona pack must contain persona_piece module")
    header = manifest.get("header", {})
    try:
        for identifier in (header["uuid"], module["uuid"]):
            if not isinstance(identifier, str) or str(uuid.UUID(identifier)) != identifier.lower():
                raise ValueError("non-canonical UUID")
    except (KeyError, ValueError, TypeError) as exc:
        raise ValueError("native Persona header/module uuid must be canonical UUID") from exc
    version = header.get("version")
    valid_version = (isinstance(version, list) and len(version) == 3
        and all(isinstance(n, int) and not isinstance(n, bool) and n >= 0 for n in version))
    valid_version = valid_version or (isinstance(version, str)
        and bool(re.fullmatch(r"\d+(\.\d+)+", version)))
    if not valid_version:
        raise ValueError("native Persona header.version must be 3 integers or dotted string")
    return manifest
def safe_filename(name):
    base = re.sub(r'[<>:"/\\|?*\x00-\x1f]', '_', name).strip('. ')
    if not base:
        raise ValueError("cape name must not be blank")
    return base
