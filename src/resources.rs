//! Small, self-authored Cosmos interoperability metadata embedded in DLL.
//! These are NOT Microsoft entitlements or third-party Persona ZIP binaries.
pub fn capes_json() -> &'static [u8] {
    include_bytes!("../overrides/capes_items.json")
}

pub fn catalog_json(id: &str) -> Option<&'static [u8]> {
    match id.to_ascii_lowercase().as_str() {
        "5bba5540-377b-4ec2-9633-4551c4e82e37" =>
            Some(include_bytes!("../overrides/catalog/5bba5540-377b-4ec2-9633-4551c4e82e37.json")),
        "e847fc66-b988-4065-bdc9-fa8fd17457b2" =>
            Some(include_bytes!("../overrides/catalog/e847fc66-b988-4065-bdc9-fa8fd17457b2.json")),
        _ => None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn built_in_capes_are_not_owned() {
        let list:serde_json::Value=serde_json::from_slice(capes_json()).unwrap();
        let items=list.as_array().unwrap();
        assert_eq!(items.len(),2);
        for item in items {
            assert_eq!(item["ownership"],"NotOwned");
            let id=item["id"].as_str().unwrap();
            let catalog:serde_json::Value=serde_json::from_slice(catalog_json(id).unwrap()).unwrap();
            assert_eq!(catalog["data"]["Item"]["Id"],id);
        }
    }
}
