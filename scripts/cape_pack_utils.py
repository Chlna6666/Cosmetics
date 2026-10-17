"""Shared, dependency-free validation for Cosmetics authoring ZIPs.

This validates author input; native Persona archives are preserved, never signed
or modified. Rendering of animation inside a Persona bundle is game-dependent.
"""
import binascii
import json
from pathlib import Path
import re
import struct
import uuid
from zipfile import ZipFile, BadZipFile

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
MAX_PNG = 8 * 1024 * 1024
MAX_NATIVE = 64 * 1024 * 1024
MAX_FILES = 256
MAX_OUTER = 64 * 1024 * 1024
ALLOWED_RARITIES = {"common", "uncommon", "rare", "epic", "legendary"}


def check_png(data):
    """Check PNG structure/CRCs and Bedrock cape UV dimensions.

    Does not enlarge a 64x32 image: HD capes retain their source pixels.
    """
    if len(data) > MAX_PNG:
        raise ValueError("PNG exceeds 8 MiB")
    if len(data) < 57 or not data.startswith(PNG_SIGNATURE):
        raise ValueError("input is not a complete PNG")
    cursor = len(PNG_SIGNATURE)
    dimensions = None
    found_data = False
    end = False
    while cursor + 12 <= len(data):
        size = struct.unpack_from(">I", data, cursor)[0]
        tag = data[cursor + 4:cursor + 8]
        endpos = cursor + 12 + size
        if endpos > len(data):
            raise ValueError("truncated PNG chunk")
        content = data[cursor + 8:cursor + 8 + size]
        crc = struct.unpack_from(">I", data, cursor + 8 + size)[0]
        if binascii.crc32(tag + content) & 0xffffffff != crc:
            raise ValueError("PNG chunk CRC mismatch")
        if dimensions is None:
            if tag != b"IHDR" or size != 13:
                raise ValueError("missing PNG IHDR")
            width, height, depth, color, compress, flt, interlace = struct.unpack(">IIBBBBB", content)
            if (depth not in (1, 2, 4, 8, 16) or color not in (0, 2, 3, 4, 6)
                or compress != 0 or flt != 0 or interlace not in (0, 1)):
                raise ValueError("unsupported PNG encoding")
            if (width < 64 or height < 32 or width > 2048 or height > 2048
                    or width % 64 or height % 32 or width // 64 != height // 32):
                raise ValueError(f"invalid cape UV dimensions {width}x{height}; expected 64x32 multiplied by 1..32")
            dimensions = (width, height)
        if tag == b"IDAT":
            found_data = True
        if tag == b"IEND":
            if size or endpos != len(data):
                raise ValueError("invalid PNG IEND")
            end = True
            break
        cursor = endpos
    if not end or not found_data:
        raise ValueError("PNG missing IDAT/IEND")
    return dimensions


def texture_info(width, height):
    return {"width": width, "height": height, "scale": width // 64, "uv": "bedrock_64x32"}


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
            if len(outer.infolist()) > MAX_FILES:
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
    if not isinstance(modules, list) or not any(
            isinstance(m, dict) and m.get("type") == "persona_piece" for m in modules):
        raise ValueError("native Persona pack must contain persona_piece module")
    header = manifest.get("header", {})
    try:
        uuid.UUID(header["uuid"])
    except (KeyError, ValueError, TypeError) as exc:
        raise ValueError("native Persona header.uuid must be UUID") from exc
    return manifest
def safe_filename(name):
    base = re.sub(r'[<>:"/\\|?*\x00-\x1f]', '_', name).strip('. ')
    if not base:
        raise ValueError("cape name must not be blank")
    return base
