"""Offline checks for the opt-in Cosmos mapping importer."""
import unittest
import tempfile
import json
from pathlib import Path
from unittest.mock import patch
from import_cosmos import choose_items, load_json, parse_mappings, safe_zip_url, unique_cape_items, import_catalog


class ImportTests(unittest.TestCase):
    def test_mapping_comments_and_paths(self):
        raw = b'[\n// comment-only line\n{"uuid":"5bba5540-377b-4ec2-9633-4551c4e82e37","response":"Persona\\\\CustomCapes\\\\AndroidCape.json"}\n]'
        value = load_json(raw)
        mapping = parse_mappings(value)
        self.assertEqual(mapping["5bba5540-377b-4ec2-9633-4551c4e82e37"],
                         "Persona/CustomCapes/AndroidCape.json")

    def test_cape_items_are_not_marked_purchased(self):
        cape = {"id":"5bba5540-377b-4ec2-9633-4551c4e82e37",
                "title":"Cape", "pieceType":"persona_capes",
                "ownership":"Purchased", "receipt":"secret"}
        page = {"result":{"layout":[{"rows":[{"components":[{"type":"itemListComp",
                                                               "items":[cape]}]}]}]}}
        items = list(unique_cape_items(page))
        mapped = choose_items(items, {cape["id"]:"Persona/CustomCapes/AndroidCape.json"},
                              10, "")
        self.assertEqual(mapped[0]["ownership"],"NotOwned")
        self.assertNotIn("receipt",mapped[0])
        self.assertEqual(cape["ownership"],"Purchased")

    def test_unbounded_limit_imports_all_mapped_capelines(self):
        uid=lambda i: f"00000000-0000-4000-8000-{i:012d}"
        items=[{"id":uid(i),"title":f"Cape {i}","pieceType":"persona_capes",
                "ownership":"Purchased"} for i in range(1,46)]
        mapping={item["id"]:f"Persona/CustomCapes/Cape_{i}.json"
                 for i,item in enumerate(items,1)}
        self.assertEqual(len(choose_items(items,mapping,None,"")),45)
        self.assertEqual(len(choose_items(items,mapping,30,"")),30)

    def test_importer_copies_every_valid_mapped_item(self):
        uid=lambda i: f"00000000-0000-4000-8000-{i:012d}"
        items=[{"id":uid(i),"title":f"Cape {i}","pieceType":"persona_capes",
                "ownership":"Purchased"} for i in range(1,36)]
        page={"result":{"layout":[{"rows":[{"components":[{
            "type":"itemListComp","items":items}]}]}]}}
        mapping=[{"uuid":item["id"],"response":f"Persona/CustomCapes/Cape_{i}.json"}
                 for i,item in enumerate(items,1)]
        def mocked_remote(path):
            if path=="MainPages/Capes.json": return json.dumps(page).encode()
            if path=="LauncherJsons/PlayfabGetPublishItemResponses.json":
                return json.dumps(mapping).encode()
            index=int(path.rsplit("_",1)[1].removesuffix(".json"))
            entry={"data":{"Item":{"Id":uid(index),"Contents":[{
                "Type":"personabinary",
                "Url":f"https://capes.bedrock-cosmos.app/zips/Cape_{index}.zip"}]}}}
            return json.dumps(entry).encode()
        with tempfile.TemporaryDirectory() as td:
            with patch("import_cosmos.remote", side_effect=mocked_remote):
                import_catalog(td,max_count=None,workers=4)
            found=json.loads((Path(td)/"overrides/capes_items.json").read_text())
            self.assertEqual(len(found),35)
            self.assertTrue(all(x["ownership"]=="NotOwned" for x in found))
            self.assertEqual(len(list((Path(td)/"overrides/catalog").glob("*.json"))),35)

    def test_only_allowlisted_cdns(self):
        self.assertTrue(safe_zip_url("https://capes.bedrock-cosmos.app/zips/AndroidCape.zip"))
        self.assertFalse(safe_zip_url("https://example.com/zips/AndroidCape.zip"))
        self.assertFalse(safe_zip_url("https://capes.bedrock-cosmos.app/zips/../a.zip"))
        self.assertFalse(safe_zip_url("https://capes.bedrock-cosmos.app/zips/a.zip?token=secret"))


if __name__ == "__main__":
    unittest.main()
