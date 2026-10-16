#!/usr/bin/env python3
"""Opt-in Cosmos response importer. No third-party bytes are redistributed
with Cosmetics. This script runs on the user's computer, not in release CI."""
import argparse
import json
import re
import sys
import urllib.request
import urllib.error
from pathlib import Path
from urllib.parse import quote, urlparse

BASE = "https://raw.githubusercontent.com/Bedrock-Cosmos/Responses/main/"
COSMOS_ZIP_HOST = "capes.bedrock-cosmos.app"
MAX_JSON = 4 * 1024 * 1024
MAX_ZIP = 64 * 1024 * 1024
UUID = re.compile(r"^[0-9a-fA-F]{8}-(?:[0-9a-fA-F]{4}-){3}[0-9a-fA-F]{12}$")


def load_json(content):
    # Upstream mappings contain comment-only // lines, not valid strict JSON.
    text = "\n".join(line for line in content.decode("utf-8-sig").splitlines()
                     if not line.lstrip().startswith("//"))
    return json.loads(text)


def remote(path, limit=MAX_JSON):
    if not path or any(x in path for x in ("..", "?", "#", "\\")):
        raise ValueError("unsafe upstream response path")
    if not re.fullmatch(r"[A-Za-z0-9_./! -]+", path):
        raise ValueError("unsupported upstream path characters")
    url = BASE + quote(path, safe="/")
    with urllib.request.urlopen(urllib.request.Request(url, headers={
        "User-Agent": "Cosmetics-response-importer/0.1"
    }), timeout=20) as response:
        body = response.read(limit + 1)
    if len(body) > limit:
        raise ValueError("response exceeds size bound")
    return body


def unique_cape_items(page):
    layouts = page.get("result", {}).get("layout", [])
    for layout in layouts:
        for row in layout.get("rows", []):
            for component in row.get("components", []):
                if component.get("type") != "itemListComp":
                    continue
                for item in component.get("items", []):
                    if item.get("pieceType") == "persona_capes":
                        yield item


def choose_items(items, mapping, limit, keyword):
    seen = set()
    selected = []
    keyword = keyword.casefold()
    for item in items:
        item_id = item.get("id", "")
        title = item.get("title", "")
        if not UUID.fullmatch(item_id) or item_id not in mapping or item_id in seen:
            continue
        if keyword and keyword not in str(title).casefold():
            continue
        seen.add(item_id)
        # The upstream response may claim Purchased. Do not impersonate the
        # user's Microsoft inventory or ownership, even for official capes.
        copy = json.loads(json.dumps(item))
        copy["ownership"] = "NotOwned"
        copy.pop("receipt", None)
        copy.pop("purchaseDate", None)
        selected.append(copy)
        if len(selected) >= limit:
            break
    return selected


def parse_mappings(data):
    out = {}
    for row in data:
        item_id = row.get("uuid")
        path = row.get("response", "").replace("\\", "/")
        if UUID.fullmatch(item_id or "") and path.startswith("Persona/") and path.endswith(".json"):
            out[item_id] = path
    return out


def safe_zip_url(value):
    parsed = urlparse(value)
    return (parsed.scheme == "https" and parsed.netloc == COSMOS_ZIP_HOST
            and parsed.path.startswith("/zips/")
            and re.fullmatch(r"[A-Za-z0-9_.-]+\.zip", parsed.path.removeprefix("/zips/"))
            and not parsed.query and not parsed.fragment)


def validate_catalog(item_id, catalog):
    entry = catalog.get("data", {}).get("Item", {})
    if entry.get("Id", "").lower() != item_id.lower():
        raise ValueError("mismatched PlayFab ID")
    for content in entry.get("Contents", []):
        if content.get("Type") != "personabinary" or not safe_zip_url(content.get("Url", "")):
            raise ValueError("catalog has unsupported remote content")
    if not entry.get("Contents"):
        raise ValueError("catalog missing personabinary contents")


def import_catalog(directory, max_count, keyword, bundle_zips=False):
    directory = Path(directory).resolve()
    # Do not overwrite the live JSON in place until upstream files are valid.
    page = load_json(remote("MainPages/Capes.json"))
    mapping = parse_mappings(load_json(remote("LauncherJsons/PlayfabGetPublishItemResponses.json")))
    selected = choose_items(unique_cape_items(page), mapping, max_count, keyword)
    checked = []
    for item in selected:
        item_id = item["id"]
        try:
            response = load_json(remote(mapping[item_id]))
            validate_catalog(item_id, response)
            checked.append((item, response))
        except (ValueError, KeyError, OSError, urllib.error.URLError) as exc:
            print(f"Skipping {item.get('title', 'unknown')!r}: {exc}", file=sys.stderr)
    if not checked:
        raise SystemExit("No usable cape records found; no local files changed")
    catalog_dir = directory / "overrides" / "catalog"
    catalog_dir.mkdir(parents=True, exist_ok=True)
    for item, response in checked:
        (catalog_dir / (item["id"] + ".json")).write_text(
            json.dumps(response, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    # Preserve existing user-added capes; replace only the matching items.
    capes_file = directory / "overrides" / "capes_items.json"
    existing = json.loads(capes_file.read_text("utf-8")) if capes_file.exists() else []
    if not isinstance(existing, list):
        raise ValueError("existing capes_items.json must be an array")
    dedup = {item["id"]: item for item in existing if isinstance(item, dict)
             and isinstance(item.get("id"), str)}
    for item, _ in checked:
        if item["id"] in dedup and dedup[item["id"]].get("ownership") == "Purchased":
            continue
        dedup[item["id"]] = item
    capes_file.parent.mkdir(parents=True, exist_ok=True)
    capes_file.write_text(json.dumps(list(dedup.values()), indent=2,
                                     ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"Imported {len(checked)} Cosmos catalog entries into {directory}")
    print("ZIP resources still use Cosmos' normal HTTPS CDN. No entitlement changed.")
    if bundle_zips:
        directory_zip = directory / "overrides" / "persona"
        directory_zip.mkdir(parents=True, exist_ok=True)
        for item, response in checked:
            for content in response["data"]["Item"]["Contents"]:
                resource_id = content.get("Id", "")
                if not UUID.fullmatch(resource_id):
                    print("Skipping ZIP with no valid Contents.Id", file=sys.stderr)
                    continue
                url = content["Url"]
                with urllib.request.urlopen(url, timeout=30) as source:
                    data = source.read(MAX_ZIP + 1)
                if not data.startswith(b"PK\x03\x04") or len(data) > MAX_ZIP:
                    print(f"Skipping invalid/oversized ZIP for {item['title']}", file=sys.stderr)
                    continue
                (directory_zip / f"{resource_id}.zip").write_bytes(data)
                content["Url"] = (
                    f"https://cosmetics.invalid/personabinary/{resource_id}.zip")
            (catalog_dir / (item["id"] + ".json")).write_text(
                json.dumps(response, indent=2) + "\n", encoding="utf-8")
        print("Local ZIP files downloaded. This does NOT prove Persona validation or equipability.")


def main():
    parser = argparse.ArgumentParser(description="Import Cosmos JSON metadata for local-only Cosmetics use.")
    parser.add_argument("--destination", default=".", help="Directory containing cosmetics.json")
    parser.add_argument("--limit", type=int, default=15, help="Max imported capes (1-100)")
    parser.add_argument("--name", default="", help="Only titles containing this text")
    parser.add_argument("--bundle-zips", action="store_true",
                        help="Download upstream ZIPs into local overrides/persona for use on this machine")
    parser.add_argument("--confirm-asset-rights", action="store_true",
                        help="Confirm you have permission to download/store requested third-party assets")
    args = parser.parse_args()
    if not 1 <= args.limit <= 100:
        parser.error("--limit must be 1..100")
    if args.bundle_zips and not args.confirm_asset_rights:
        parser.error("--bundle-zips requires --confirm-asset-rights")
    import_catalog(args.destination, args.limit, args.name, args.bundle_zips)


if __name__ == "__main__":
    main()
