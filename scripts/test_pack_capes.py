"""Multi-cape authoring tests: IDs are created once, stored and reused."""
import importlib.util
import json
from pathlib import Path
import struct
import zlib
import binascii
import io
import uuid
import tempfile
import unittest
from zipfile import ZipFile

spec=importlib.util.spec_from_file_location("pack_capes",Path(__file__).with_name("pack_capes.py"))
module=importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

def png_header(width=64,height=32):
    def chunk(kind,raw):
        return struct.pack(">I",len(raw))+kind+raw+struct.pack(">I",binascii.crc32(kind+raw)&0xffffffff)
    return (b"\x89PNG\r\n\x1a\n"
        +chunk(b"IHDR",struct.pack(">IIBBBBB",width,height,8,6,0,0,0))
        +chunk(b"IDAT",zlib.compress((b"\x00"+b"\x22\x66\x88\xff"*width)*height))
        +chunk(b"IEND",b""))

class MultiCapePackTests(unittest.TestCase):
    def test_single_zip_holds_three_capes_with_permanent_ids(self):
        with tempfile.TemporaryDirectory() as td:
            source=Path(td)/"skins"
            source.mkdir()
            for name in ["red","blue","我的披风"]:
                (source/f"{name}.png").write_bytes(png_header())
            target=Path(td)/"capes"/"collection.zip"
            module.create(source,target,name="Our Collection",author="Example")
            persisted=json.loads((source/"pack.json").read_text(encoding="utf-8"))
            self.assertEqual(persisted["format_version"],1)
            self.assertEqual(len(persisted["capes"]),3)
            initial_ids={x["id"]:x["uuid"] for x in persisted["capes"]}
            self.assertEqual(len(set(initial_ids.values())),3)
            with ZipFile(target) as zipfile:
                self.assertIn("pack.json",zipfile.namelist())
                self.assertEqual(len(zipfile.namelist()),4)
                self.assertIn("textures/我的披风.png",zipfile.namelist())
            module.create(source,target,author="Updated Artist")
            again=json.loads((source/"pack.json").read_text(encoding="utf-8"))
            self.assertEqual(again["id"],persisted["id"])
            self.assertEqual({x["id"]:x["uuid"] for x in again["capes"]},initial_ids)
            module.create(source,target,version="1.0.1")
            next_version=json.loads((source/"pack.json").read_text(encoding="utf-8"))
            self.assertEqual(next_version["version"],"1.0.1")
            self.assertEqual(next_version["id"],persisted["id"])
            self.assertEqual({x["id"]:x["uuid"] for x in next_version["capes"]},initial_ids)
    def test_pack_extends_with_new_cape_without_rewriting_old_ids(self):
        with tempfile.TemporaryDirectory() as td:
            folder=Path(td)/"pack"
            folder.mkdir()
            (folder/"alpha.png").write_bytes(png_header())
            output=Path(td)/"out.zip"
            module.create(folder,output)
            first=json.loads((folder/"pack.json").read_text())["capes"][0]
            (folder/"beta.png").write_bytes(png_header())
            module.create(folder,output)
            entries=json.loads((folder/"pack.json").read_text())["capes"]
            self.assertEqual(len(entries),2)
            self.assertIn(first,entries)
    def test_bad_png_does_not_overwrite_persistent_manifest(self):
        with tempfile.TemporaryDirectory() as td:
            folder=Path(td)/"pack"
            folder.mkdir()
            (folder/"good.png").write_bytes(png_header())
            module.create(folder,Path(td)/"one.zip")
            manifest=(folder/"pack.json").read_bytes()
            (folder/"bad.png").write_bytes(b"not an image")
            with self.assertRaises(ValueError):
                module.create(folder,Path(td)/"one.zip")
            self.assertEqual((folder/"pack.json").read_bytes(),manifest)

    def test_hd_png_is_preserved_without_python_resolution_metadata(self):
        with tempfile.TemporaryDirectory() as td:
            folder=Path(td)/"hires"
            folder.mkdir()
            (folder/"4k.png").write_bytes(png_header(1280,640))
            destination=Path(td)/"collection.zip"
            module.create(folder,destination,rarity="legendary")
            meta=json.loads((folder/"pack.json").read_text())
            self.assertNotIn("texture_info",meta["capes"][0])
            with ZipFile(destination) as archive:
                self.assertEqual(archive.read("textures/4k.png"),(folder/"4k.png").read_bytes())
            self.assertEqual(meta["capes"][0]["rarity"],"legendary")
            persisted=meta["capes"][0]["uuid"]
            module.create(folder,destination)
            self.assertEqual(json.loads((folder/"pack.json").read_text())["capes"][0]["uuid"],persisted)
    def test_native_animation_payload_is_preserved_and_static_preview_optional(self):
        with tempfile.TemporaryDirectory() as td:
            folder=Path(td)/"animated"
            folder.mkdir()
            pack=io.BytesIO()
            with ZipFile(pack,"w") as archive:
                archive.writestr("manifest.json",json.dumps({
                    "format_version":1,
                    "header":{"uuid":str(uuid.uuid4()),"version":[1,0,0]},
                    "modules":[{"type":"persona_piece","uuid":str(uuid.uuid4())}]}))
                archive.writestr("animation/frames.json",b'{"frames":3}')
            outer=io.BytesIO()
            with ZipFile(outer,"w") as archive:
                archive.writestr("ppack0.zip",pack.getvalue())
            original=outer.getvalue()
            (folder/"foo.persona.zip").write_bytes(original)
            target=Path(td)/"output.zip"
            module.create(folder,target)
            with ZipFile(target) as archive:
                capes=json.loads(archive.read("pack.json"))["capes"]
                self.assertEqual(len(capes),1)
                self.assertNotIn("texture",capes[0])
                self.assertEqual(archive.read("native/foo.zip"),original)
            (folder/"foo.png").write_bytes(png_header(128,64))
            module.create(folder,target)
            with ZipFile(target) as archive:
                capes=json.loads(archive.read("pack.json"))["capes"]
                self.assertEqual(len(capes),1)
                self.assertEqual(archive.read("native/foo.zip"),original)
                self.assertNotIn("texture_info",capes[0])
                self.assertEqual(archive.read("textures/foo.png"),(folder/"foo.png").read_bytes())
    def test_invalid_png_header_does_not_replace_old_multi_zip(self):
        with tempfile.TemporaryDirectory() as td:
            folder=Path(td)/"source"
            folder.mkdir()
            file=folder/"ok.png"
            file.write_bytes(png_header())
            target=Path(td)/"output.zip"
            module.create(folder,target)
            original=target.read_bytes()
            manifest=(folder/"pack.json").read_bytes()
            file.write_bytes(b"not a png")
            with self.assertRaisesRegex(ValueError,"PNG"):
                module.create(folder,target)
            self.assertEqual(original,target.read_bytes())
            self.assertEqual(manifest,(folder/"pack.json").read_bytes())

if __name__=="__main__":
    unittest.main()
