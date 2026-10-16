"""Tests ZIP packaging convenience without Minecraft or network access."""
import importlib.util
import json
from pathlib import Path
import struct
import tempfile
import unittest
from zipfile import ZipFile

spec=importlib.util.spec_from_file_location(
    "pack_cape",Path(__file__).with_name("pack_cape.py"))
module=importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

def png_header():
    return b"\x89PNG\r\n\x1a\n"+struct.pack(">I",13)+b"IHDR"+struct.pack(">II",64,32)+b"\x08\x06\x00\x00\x00"+b"\x00\x00\x00\x00"

class PackTests(unittest.TestCase):
    def test_one_png_generates_one_self_contained_zip(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)
            png=root/"input.png"
            png.write_bytes(png_header())
            out=module.create(png,root/"capes"/"demo.zip",name="Local test",author="Me")
            with ZipFile(out) as archive:
                self.assertEqual(set(archive.namelist()),{"cape.json","cape.png"})
                self.assertEqual(archive.read("cape.png"),png.read_bytes())
                meta=json.loads(archive.read("cape.json"))
                self.assertEqual(meta["name"],"Local test")
                self.assertEqual(meta["author"],"Me")
    def test_reject_non_png(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)
            png=root/"input.png"
            png.write_bytes(b"not a png")
            with self.assertRaises(ValueError):
                module.create(png,root/"capes"/"bad.zip")
if __name__=="__main__":
    unittest.main()
