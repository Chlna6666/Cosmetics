"""Offline checks for the opt-in Cosmos mapping importer."""
import unittest
from import_cosmos import choose_items, load_json, parse_mappings, safe_zip_url, unique_cape_items


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

    def test_only_allowlisted_cdns(self):
        self.assertTrue(safe_zip_url("https://capes.bedrock-cosmos.app/zips/AndroidCape.zip"))
        self.assertFalse(safe_zip_url("https://example.com/zips/AndroidCape.zip"))
        self.assertFalse(safe_zip_url("https://capes.bedrock-cosmos.app/zips/../a.zip"))
        self.assertFalse(safe_zip_url("https://capes.bedrock-cosmos.app/zips/a.zip?token=secret"))


if __name__ == "__main__":
    unittest.main()
