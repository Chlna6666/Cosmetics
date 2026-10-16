#!/usr/bin/env python3
"""Opt-in Cosmos response importer. No third-party bytes are redistributed
with Cosmetics. This script runs on the user's computer, not in release CI."""
import argparse
from concurrent.futures import ThreadPoolExecutor
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
        if limit is not None and len(selected) >= limit:
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


def import_catalog(directory, max_count=None, keyword="", bundle_zips=False, include_unmapped=False,
                   workers=8):
    directory = Path(directory).resolve()
    page = load_json(remote("MainPages/Capes.json"))
    mapping = parse_mappings(load_json(remote("LauncherJsons/PlayfabGetPublishItemResponses.json")))
    all_items = list(unique_cape_items(page))
    available = {item.get("id") for item in all_items if UUID.fullmatch(item.get("id", ""))}
    missing = available - set(mapping)
    selected = choose_items(all_items, mapping, max_count, keyword)
    print(f"Cosmos page: {len(available)} unique capes; {len(available & set(mapping))} mapped "
          f"to catalog metadata, {len(missing)} have no published local mapping.", flush=True)
    print(f"Fetching {len(selected)} catalog JSON files, workers={workers} "
          f"(--limit 0 means all mapped entries).", flush=True)

    def fetch_pair(item):
        item_id=item["id"]
        try:
            response=load_json(remote(mapping[item_id]))
            validate_catalog(item_id,response)
            return (item,response,None)
        except (ValueError,KeyError,OSError,urllib.error.URLError,TimeoutError) as exc:
            return (item,None,str(exc))

    checked=[]
    failures=0
    with ThreadPoolExecutor(max_workers=workers) as executor:
        for index,(item,response,error) in enumerate(executor.map(fetch_pair,selected),1):
            if response is not None:
                checked.append((item,response))
            else:
                failures+=1
                print(f"Skipping {item.get('title','unknown')!r}: {error}",file=sys.stderr)
            if index%25==0 or index==len(selected):
                print(f"  validated {index}/{len(selected)}: successful={len(checked)}, "
                      f"failed={failures}",flush=True)
    if not checked and not include_unmapped:
        raise SystemExit("No usable cape records found; no local files changed")

    # Only explicit local metadata. Official purchased items are never
    # rewritten and the game's actual entitlement/inventory remains untouched.
    catalog_dir=directory/"overrides"/"catalog"
    catalog_dir.mkdir(parents=True,exist_ok=True)
    for item,response in checked:
        (catalog_dir/(item["id"]+".json")).write_text(
            json.dumps(response,indent=2,ensure_ascii=False)+"\n",encoding="utf-8")
    cape_file=directory/"overrides"/"capes_items.json"
    existing=json.loads(cape_file.read_text("utf-8")) if cape_file.exists() else []
    if not isinstance(existing,list):
        raise ValueError("existing capes_items.json must be an array")
    by_id={item["id"]:item for item in existing
           if isinstance(item,dict) and isinstance(item.get("id"),str)}
    for item,_ in checked:
        if by_id.get(item["id"],{}).get("ownership")=="Purchased":
            continue
        by_id[item["id"]]=item
    if include_unmapped:
        for item in all_items:
            id=item.get("id","")
            if not UUID.fullmatch(id) or id in mapping or (keyword and keyword.casefold() not in str(item.get("title","")).casefold()):
                continue
            copy=json.loads(json.dumps(item))
            copy["ownership"]="NotOwned"
            copy.pop("receipt",None)
            copy.pop("purchaseDate",None)
            if by_id.get(id,{}).get("ownership")!="Purchased":
                by_id[id]=copy
    cape_file.parent.mkdir(parents=True,exist_ok=True)
    cape_file.write_text(json.dumps(list(by_id.values()),indent=2,ensure_ascii=False)+"\n",
                         encoding="utf-8")
    print(f"Imported {len(checked)} verified Cosmos catalog records; "
          f"local menu now has {len(by_id)} entries.",flush=True)
    if missing:
        print(f"NOTE: {len(missing)} upstream cape entries have no listed local catalog mapping."
              " Use --include-unmapped to display them with official PlayFab fallback;"
              " they are not guaranteed to load.",flush=True)
    if failures:
        print(f"WARNING: {failures} item(s) failed metadata validation; retry importer.",flush=True)
    print("ZIP assets still use Cosmos HTTPS CDN; no Microsoft entitlements changed.",flush=True)

    if bundle_zips:
        directory_zip=directory/"overrides"/"persona"
        directory_zip.mkdir(parents=True,exist_ok=True)
        for item,response in checked:
            for content in response["data"]["Item"]["Contents"]:
                resource_id=content.get("Id","")
                if not UUID.fullmatch(resource_id):
                    print("Skipping ZIP without a valid Contents.Id",file=sys.stderr)
                    continue
                url=content["Url"]
                try:
                    with urllib.request.urlopen(url,timeout=30) as source:
                        data=source.read(MAX_ZIP+1)
                    if not data.startswith(b"PK\x03\x04") or len(data)>MAX_ZIP:
                        raise ValueError("invalid or oversized ZIP")
                    (directory_zip/f"{resource_id}.zip").write_bytes(data)
                    content["Url"]=f"https://cosmetics.invalid/personabinary/{resource_id}.zip"
                except (ValueError,OSError,urllib.error.URLError) as exc:
                    print(f"ZIP skipped for {item['title']!r}: {exc}",file=sys.stderr)
            (catalog_dir/(item["id"]+".json")).write_text(
                json.dumps(response,indent=2,ensure_ascii=False)+"\n",encoding="utf-8")
        print("Local ZIP download completed; in-game Persona validation remains unverified.")

def main():
    parser = argparse.ArgumentParser(description="Import Cosmos JSON metadata for local-only Cosmetics use.")
    parser.add_argument("--destination", default=".", help="Directory containing cosmetics.json")
    parser.add_argument("--limit", type=int, default=0, help="Maximum matched capes to import; 0 = all (default)")
    parser.add_argument("--all", action="store_true", help="Import every mapped cape (same as --limit 0)")
    parser.add_argument("--include-unmapped", action="store_true",
                        help="Also display upstream cape IDs without local PlayFab mappings; may be unusable")
    parser.add_argument("--workers", type=int, default=8, help="Parallel metadata requests (1-16)")
    parser.add_argument("--name", default="", help="Only titles containing this text")
    parser.add_argument("--bundle-zips", action="store_true",
                        help="Download upstream ZIPs into local overrides/persona for use on this machine")
    parser.add_argument("--confirm-asset-rights", action="store_true",
                        help="Confirm you have permission to download/store requested third-party assets")
    args = parser.parse_args()
    if not 0 <= args.limit <= 10000:
        parser.error("--limit must be 0..10000")
    if not 1 <= args.workers <= 16:
        parser.error("--workers must be 1..16")
    if args.bundle_zips and not args.confirm_asset_rights:
        parser.error("--bundle-zips requires --confirm-asset-rights")
    import_catalog(args.destination, None if args.all or args.limit==0 else args.limit,
                   args.name,args.bundle_zips,args.include_unmapped,args.workers)


if __name__ == "__main__":
    main()
