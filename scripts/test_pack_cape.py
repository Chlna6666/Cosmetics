"""Offline tests for the static-HD and native-Persona cape authoring entry point."""
import importlib.util
import io
import json
from pathlib import Path
import struct
import sys
import tempfile
import unittest
import uuid
from zipfile import ZipFile
import zlib
import binascii

sys.path.insert(0,str(Path(__file__).resolve().parent))
spec=importlib.util.spec_from_file_location("pack_cape",Path(__file__).with_name("pack_cape.py"))
module=importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def chunk(kind,payload):
    return (struct.pack(">I",len(payload))+kind+payload+
            struct.pack(">I",binascii.crc32(kind+payload)&0xffffffff))


def png_bytes(w=64,h=32):
    pixel=b"\xff\x20\x40\xff"
    return (b"\x89PNG\r\n\x1a\n"+
            chunk(b"IHDR",struct.pack(">IIBBBBB",w,h,8,6,0,0,0))+
            chunk(b"IDAT",zlib.compress((b"\x00"+pixel*w)*h))+
            chunk(b"IEND",b""))


def bundled_native():
    pack_uuid=str(uuid.uuid4())
    inner=io.BytesIO()
    with ZipFile(inner,"w") as archive:
        archive.writestr("manifest.json",json.dumps({
            "format_version":1,"header":{"uuid":pack_uuid,"version":[1,0,0]},
            "modules":[{"type":"persona_piece","uuid":str(uuid.uuid4()),"version":[1,0,0]}]}))
        archive.writestr("animation/frames.json",b'{"frames":2,"fps":4}')
        archive.writestr("animation/frame_0001.png",png_bytes())
    bundle=io.BytesIO()
    with ZipFile(bundle,"w") as archive:
        archive.writestr("ppack0.zip",inner.getvalue())
    return bundle.getvalue()


class PackTests(unittest.TestCase):
    def test_single_png_generates_complete_metadata(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)
            png=root/"input.png"
            png.write_bytes(png_bytes())
            out=module.create(png,root/"capes"/"demo.zip",name="Local test",
                              author="Me",rarity="epic",start_date="2026-10-17T13:00:00Z")
            with ZipFile(out) as archive:
                self.assertEqual(set(archive.namelist()),{"cape.json","cape.png"})
                self.assertEqual(archive.read("cape.png"),png.read_bytes())
                meta=json.loads(archive.read("cape.json"))
                self.assertEqual(meta["name"],"Local test")
                self.assertEqual(meta["author"],"Me")
                self.assertEqual(meta["rarity"],"epic")
                self.assertNotIn("texture_info",meta)
    def test_hd_1280x640_passes_source_png_untouched_to_runtime(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)
            png=root/"HD.png"
            png.write_bytes(png_bytes(1280,640))
            out=module.create(png,root/"capes"/"hd.zip")
            with ZipFile(out) as archive:
                self.assertEqual(archive.read("cape.png"),png.read_bytes())
                meta=json.loads(archive.read("cape.json"))
                self.assertNotIn("texture_info",meta)
                self.assertEqual(meta["name"],"HD")
    def test_complete_persona_bundle_keeps_animated_resources_byte_exact(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)
            binary=bundled_native()
            input_zip=root/"sample.persona.zip"
            input_zip.write_bytes(binary)
            output=module.create(None,root/"capes"/"animated.zip",
                                 name="Animated",persona_zip=input_zip)
            with ZipFile(output) as archive:
                self.assertEqual(archive.read("persona.zip"),binary)
                self.assertNotIn("cape.png",archive.namelist())
    def test_native_many_frame_files_are_kept_untouched(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)
            inner=io.BytesIO()
            with ZipFile(inner,"w") as archive:
                archive.writestr("manifest.json",json.dumps({
                    "header":{"uuid":str(uuid.uuid4()),"version":[1,0,0]},
                    "modules":[{"type":"persona_piece","uuid":str(uuid.uuid4())}]}))
                for i in range(300):
                    archive.writestr(f"frames/{i:04}.png",b"x")
            outer=io.BytesIO()
            with ZipFile(outer,"w") as archive:
                archive.writestr("ppack0.zip",inner.getvalue())
            input_zip=root/"many.persona.zip"
            input_zip.write_bytes(outer.getvalue())
            out=module.create(None,root/"many.zip",persona_zip=input_zip)
            with ZipFile(out) as archive:
                self.assertEqual(archive.read("persona.zip"),outer.getvalue())
    def test_reject_native_missing_module_uuid(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)
            native=root/"invalid.persona.zip"
            with ZipFile(native,"w") as archive:
                archive.writestr("manifest.json",json.dumps({
                    "header":{"uuid":str(uuid.uuid4()),"version":[1,0,0]},
                    "modules":[{"type":"persona_piece"}]}))
            with self.assertRaisesRegex(ValueError,"module uuid"):
                module.create(None,root/"invalid.zip",persona_zip=native)
    def test_simple_png_validation_defers_uv_and_decode_to_cosmetics(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)
            png=root/"input.png"
            png.write_bytes(b"not a png")
            with self.assertRaises(ValueError):
                module.create(png,root/"bad.zip")
            # Not a valid cape UV: Python merely archives the source.
            # The Rust DLL checks/denies this image on startup.
            png.write_bytes(png_bytes(64,64))
            result=module.create(png,root/"archived.zip")
            with ZipFile(result) as archive:
                self.assertEqual(archive.read("cape.png"),png.read_bytes())
            # Even broken PNG internals are rejected by Cosmetics' decoder,
            # never silently rewritten or 'fixed' by Python.
            bad=bytearray(png_bytes())
            bad[-5] ^= 0x08
            png.write_bytes(bad)
            result=module.create(png,root/"unchecked.zip")
            with ZipFile(result) as archive:
                self.assertEqual(archive.read("cape.png"),bytes(bad))
    def test_invalid_rarity_and_dates_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)
            png=root/"input.png"
            png.write_bytes(png_bytes())
            with self.assertRaises(ValueError):
                module.create(png,root/"bad.zip",rarity="superrare")
            with self.assertRaises(ValueError):
                module.create(png,root/"bad.zip",start_date="2026-02-30T09:00:00Z")


if __name__=="__main__":
    unittest.main()
