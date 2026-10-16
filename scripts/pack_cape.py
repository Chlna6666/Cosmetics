#!/usr/bin/env python3
"""Create one self-contained local Cosmetics cape ZIP; standard library only."""
import argparse
import json
from pathlib import Path
import re
import struct
import uuid
from zipfile import ZipFile, ZIP_DEFLATED

PNG=b"\x89PNG\r\n\x1a\n"
def check_png(data):
    if len(data)<33 or data[:8]!=PNG or data[12:16]!=b"IHDR":
        raise ValueError("input is not a PNG image")
    width,height=struct.unpack(">II",data[16:24])
    if not 0<width<=2048 or not 0<height<=2048:
        raise ValueError("invalid PNG dimensions")
    if len(data)>8*1024*1024:raise ValueError("PNG exceeds 8 MiB")
    return width,height

def create(source,output,name=None,author="Player",item_id=None,persona_zip=None):
    source=Path(source)
    data=source.read_bytes() if source.is_file() else b""
    if data:check_png(data)
    if not data and not persona_zip:
        raise ValueError("provide a PNG or --persona-zip")
    title=name or source.stem
    if not title or len(title)>90 or not author or len(author)>90:
        raise ValueError("invalid name/author")
    manifest={"name":title,"author":author}
    if item_id:
        manifest["id"]=str(uuid.UUID(item_id))
    binary=None
    if persona_zip:
        binary=Path(persona_zip).read_bytes()
        if len(binary)>64*1024*1024:
            raise ValueError("persona.zip exceeds 64 MiB")
        with ZipFile(persona_zip) as nested:
            if "manifest.json" not in nested.namelist():
                raise ValueError("native persona.zip lacks manifest.json")
            value=json.loads(nested.read("manifest.json"))
            if not any(m.get("type")=="persona_piece" for m in value.get("modules",[])):
                raise ValueError("native persona.zip lacks persona_piece")
    output=Path(output)
    output.parent.mkdir(parents=True,exist_ok=True)
    with ZipFile(output,"w",ZIP_DEFLATED) as z:
        z.writestr("cape.json",json.dumps(manifest,ensure_ascii=False,indent=2))
        if data:z.writestr("cape.png",data)
        if binary:z.writestr("persona.zip",binary)
    print(f"Created {output}: name={title!r}, image={'yes' if data else 'no'}, native_persona={'yes' if binary else 'no'}")
    print("Restart Minecraft to load the pack. PNG-only Persona equipment is experimental.")
    return output

def main():
    parser=argparse.ArgumentParser(description="Make one Cosmetics/capes/*.zip from a cape PNG.")
    parser.add_argument("png",help="Your cape.png (or an unused path with --persona-zip)")
    parser.add_argument("--name",help="Visible cape name, defaults to filename")
    parser.add_argument("--author",default="Player")
    parser.add_argument("--id",help="Optional permanent item UUID")
    parser.add_argument("--persona-zip",help="Optional prebuilt native Persona ZIP")
    parser.add_argument("--out",help="Output ZIP (defaults to capes/<name>.zip)")
    args=parser.parse_args()
    name=args.name or Path(args.png).stem
    basename=re.sub(r'[<>:"/\\|?*\x00-\x1f]','_',name).strip(". ")
    if not basename:parser.error("cape name must not be blank")
    out=args.out or f"capes/{basename}.zip"
    try:create(args.png,out,args.name,args.author,args.id,args.persona_zip)
    except (ValueError,OSError) as exc:parser.error(str(exc))

if __name__=="__main__":main()
