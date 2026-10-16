"""Multi-cape authoring tests: IDs are created once, stored and reused."""
import importlib.util
import json
from pathlib import Path
import struct
import tempfile
import unittest
from zipfile import ZipFile

spec=importlib.util.spec_from_file_location("pack_capes",Path(__file__).with_name("pack_capes.py"))
module=importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

def png_header():
    return (b"\x89PNG\r\n\x1a\n"+struct.pack(">I",13)+b"IHDR"
            +struct.pack(">II",64,32)+b"\x08\x06\x00\x00\x00"+b"\x00\x00\x00\x00")

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

if __name__=="__main__":
    unittest.main()
