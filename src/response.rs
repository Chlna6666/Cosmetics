//! Conservative JSON response overlays.
//! Never replace the user's original inventory: append-only, identity deduplicated,
//! and skip on an unrecognized schema. PlayFab item overrides are opt-in by item ID.
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

fn default_limit() -> usize { 4 * 1024 * 1024 }
fn default_method() -> String { "GET".into() }
fn default_identity() -> String { "id".into() }
fn default_enabled() -> bool { true }

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_limit")]
    pub max_capture_bytes: usize,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode { AppendArray, CatalogItem }

#[derive(Debug, Deserialize, Clone)]
pub struct Rule {
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub host: String,
    pub path: String,
    #[serde(default = "default_method")]
    pub method: String,
    pub mode: Mode,
    #[serde(default)]
    pub pointer: String,
    #[serde(default)]
    pub file: String,
    #[serde(default = "default_identity")]
    pub identity_key: String,
    #[serde(default)]
    pub catalog_directory: String,
}

impl Config {
    pub fn match_rule(&self, host: &str, path: &str, method: &str) -> Option<Rule> {
        if !self.enabled { return None; }
        self.rules.iter().find(|r| {
            r.enabled &&
            r.host.eq_ignore_ascii_case(host) &&
            r.path == path &&
            r.method.eq_ignore_ascii_case(method)
        }).cloned()
    }
}

fn guarded_path(base: &Path, child: &str) -> Result<PathBuf, String> {
    let relative = Path::new(child);
    if relative.is_absolute() || relative.components().any(|c| {
        !matches!(c, std::path::Component::Normal(_))
    }) {
        return Err("拒绝非相对安全资源路径".into());
    }
    Ok(base.join(relative))
}

/// Return None when no local overlay is configured or its item ID is unknown.
/// All errors leave the upstream response unchanged at the caller.
pub fn overlay(
    base: &Path,
    rule: &Rule,
    request_body: &[u8],
    original_body: &[u8],
) -> Result<Option<Vec<u8>>, String> {
    match rule.mode {
        Mode::AppendArray => {
            let path = guarded_path(base, &rule.file)?;
            let additions: Value = serde_json::from_slice(&fs::read(&path)
                .map_err(|e| format!("{}: {e}", path.display()))?)
                .map_err(|e| format!("overlay JSON: {e}"))?;
            let add = additions.as_array().ok_or("overlay must be a JSON array")?;
            if add.is_empty() { return Ok(None); }
            let mut root: Value = serde_json::from_slice(original_body)
                .map_err(|e| format!("original JSON invalid: {e}"))?;
            let target = root.pointer_mut(&rule.pointer)
                .and_then(Value::as_array_mut)
                .ok_or_else(|| format!("schema mismatch at {}", rule.pointer))?;
            let key = rule.identity_key.as_str();
            // Reject entries without an identity rather than silently introducing duplicates.
            let mut seen: HashSet<String> = target.iter()
                .filter_map(|value| value.get(key))
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect();
            let mut added = 0usize;
            for item in add {
                let identity = item.get(key).and_then(Value::as_str)
                    .ok_or_else(|| format!("overlay item missing string identity {key}"))?;
                if seen.insert(identity.to_string()) {
                    target.push(item.clone());
                    added += 1;
                }
            }
            if added == 0 { return Ok(None); }
            serde_json::to_vec(&root).map(Some).map_err(|e| e.to_string())
        }
        Mode::CatalogItem => {
            let request: Value = serde_json::from_slice(request_body)
                .map_err(|e| format!("catalog request JSON invalid: {e}"))?;
            let id = request.get("itemid").or_else(|| request.get("itemId"))
                .and_then(Value::as_str).ok_or("catalog request missing itemid")?;
            if !is_uuid(id) { return Ok(None); }
            let folder = guarded_path(base, &rule.catalog_directory)?;
            let path = folder.join(format!("{id}.json"));
            if !path.is_file() { return Ok(None); }
            let local: Value = serde_json::from_slice(&fs::read(&path)
                .map_err(|e| format!("{}: {e}", path.display()))?)
                .map_err(|e| format!("catalog JSON invalid: {e}"))?;
            let file_id = local.pointer("/data/Item/Id").and_then(Value::as_str)
                .ok_or("catalog response missing data.Item.Id")?;
            if !file_id.eq_ignore_ascii_case(id) {
                return Err("catalog response item ID does not match request".into());
            }
            serde_json::to_vec(&local).map(Some).map_err(|e| e.to_string())
        }
    }
}

fn is_uuid(v: &str) -> bool {
    v.len() == 36 && v.bytes().enumerate().all(|(i, b)| {
        if [8, 13, 18, 23].contains(&i) { b == b'-' }
        else { b.is_ascii_hexdigit() }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn append() -> Rule {
        Rule { name: "inventory".into(), enabled: true,
            host: "example.test".into(), path: "/inventory".into(), method: "GET".into(),
            mode: Mode::AppendArray, pointer: "/result/items".into(),
            file: "items.json".into(), identity_key: "id".into(),
            catalog_directory: String::new() }
    }
    #[test]
    fn preserves_original_entitlements_and_deduplicates() {
        let dir = std::env::temp_dir().join(format!("cosmetics_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("items.json"), r#"[{"id":"owned"},{"id":"local"}]"#).unwrap();
        let src = br#"{"result":{"items":[{"id":"owned","receipt":"genuine"}],"etag":"unchanged"}}"#;
        let value: Value = serde_json::from_slice(&overlay(&dir, &append(), b"", src)
            .unwrap().unwrap()).unwrap();
        assert_eq!(value["result"]["items"].as_array().unwrap().len(), 2);
        assert_eq!(value["result"]["items"][0]["receipt"], "genuine");
        assert_eq!(value["result"]["etag"], "unchanged");
        let _ = fs::remove_dir_all(&dir);
    }
    #[test]
    fn unknown_inventory_schema_fails_without_mutating_input() {
        let dir = std::env::temp_dir().join(format!("cosmetics_test_schema_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("items.json"), r#"[{"id":"local"}]"#).unwrap();
        let src = br#"{"result":{"other":[]}}"#;
        assert!(overlay(&dir, &append(), b"", src).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
    #[test]
    fn path_and_host_are_exact_and_catalog_id_is_validated() {
        let c: Config = serde_json::from_str(r#"{"rules":[{"name":"x","host":"store.example","path":"/p","mode":"append_array","pointer":"/items","file":"items.json"}]}"#).unwrap();
        assert!(c.match_rule("STORE.EXAMPLE", "/p", "GET").is_some());
        assert!(c.match_rule("store.example.evil", "/p", "GET").is_none());
        assert!(c.match_rule("store.example", "/p?q=1", "GET").is_none());
        assert!(is_uuid("5bba5540-377b-4ec2-9633-4551c4e82e37"));
        assert!(!is_uuid("../.."));
    }
}
