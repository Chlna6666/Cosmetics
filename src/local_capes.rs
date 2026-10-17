//! Standalone user-authored ZIP capes. No network, Cosmos, external catalog
//! files or global registration. Loaded from <Cosmetics DLL dir>/capes/*.zip.
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use image::{imageops, ImageFormat, Rgba, RgbaImage};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::sync::OnceLock;
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

const MAX_PACKS: usize=128;
const MAX_PACK_FILE: u64=64*1024*1024;
const MAX_TOTAL_BYTES:usize=128*1024*1024;
const MAX_IMAGE:usize=8*1024*1024;
const MAX_MANIFEST:usize=64*1024;
const MAX_NATIVE:usize=64*1024*1024;

// Resolution and UV are inferred by validate_png() at startup.
// Legacy texture_info JSON keys are ignored by serde, preserving old ZIPs.
#[derive(Debug,Default,Deserialize)]
struct CapeManifest {
    #[serde(default)] name:String,
    #[serde(default)] author:String,
    #[serde(default)] id:Option<String>,
    #[serde(default)] rarity:Option<String>,
    #[serde(default,rename="startDate",alias="start_date")] start_date:Option<String>,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum AssetType { PersonaZip, Png }
impl AssetType {
    pub fn content_type(self)->&'static str {
        match self{Self::PersonaZip=>"application/zip",Self::Png=>"image/png"}
    }
}
pub struct LocalAsset {
    pub data:Vec<u8>,
    pub kind:AssetType,
}
#[derive(Default)]
pub struct Registry {
    items:Vec<Value>,
    catalogs:HashMap<String,Vec<u8>>,
    assets:HashMap<String,LocalAsset>,
    pub diagnostics:Vec<String>,
    pub discovered_zip:usize,
    pub accepted:usize,
    pub rejected:usize,
    pub native_payloads:usize,
    pub generated_persona_payloads:usize,
}
static REGISTRY:OnceLock<Registry>=OnceLock::new();
pub fn initialize(root:&Path)->&'static Registry {
    REGISTRY.get_or_init(||Registry::scan(root))
}
pub fn count()->usize { REGISTRY.get().map_or(0,|r|r.items.len()) }
impl Registry {
    pub fn asset_count(&self)->usize {self.assets.len()}
}
pub fn items()->&'static [Value] {REGISTRY.get().map_or(&[],|r|r.items.as_slice())}
pub fn catalog(id:&str)->Option<&'static [u8]>{
    REGISTRY.get()?.catalogs.get(&id.to_ascii_lowercase()).map(Vec::as_slice)
}
/// Resolve only registered resources. WinHTTP request targets can carry
/// cache-busting or image-resize query parameters that are not part of the key.
pub fn canonical_asset_path(path:&str)->&str {
    path.split(['?','#']).next().unwrap_or(path)
}
pub fn asset(path:&str)->Option<(&'static [u8],AssetType)> {
    let a=REGISTRY.get()?.assets.get(canonical_asset_path(path))?;
    Some((a.data.as_slice(),a.kind))
}
fn validate_png(bytes:&[u8])->Result<RgbaImage,String>{
    // Reject truncated PNGs rather than accepting only an IHDR header.
    // A malformed cape cannot be treated as loadable after ZIP validation.
    let reader=image::load_from_memory_with_format(bytes,ImageFormat::Png)
        .map_err(|e|format!("PNG decode failed: {e}"))?;
    let (w,h)=(reader.width(),reader.height());
    if w==0||h==0||w>2048||h>2048{return Err("invalid PNG dimensions".into());}
    Ok(reader.into_rgba8())
}
/// Normalize the Persona payload texture to 8-bit RGBA, without resizing.
/// The PNG decoder supports indexed/palette (color type 3) images, but
/// Bedrock's Persona texture path has not been proven to support palette PNGs.
/// The thumbnail already uses decoded RGBA; the Persona texture must match.
/// Preserve existing RGBA8 bytes exactly to avoid needless cache churn.
fn persona_rgba_png(original:&[u8],decoded:&RgbaImage)->Result<(Vec<u8>,bool),String>{
    if original.len()>26 && original[24]==8 && original[25]==6 {
        return Ok((original.to_vec(),false));
    }
    let mut out=Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(decoded.clone())
        .write_to(&mut out,ImageFormat::Png)
        .map_err(|e|format!("Persona RGBA PNG encode failed: {e}"))?;
    if out.get_ref().len()>MAX_IMAGE {
        return Err("normalized Persona PNG exceeds 8 MiB".into());
    }
    Ok((out.into_inner(),true))
}
fn fill_thumb_rect(canvas:&mut RgbaImage,x0:u32,y0:u32,x1:u32,y1:u32,color:Rgba<u8>){
    for y in y0..y1 {
        for x in x0..x1{canvas.put_pixel(x,y,color);}
    }
}
fn make_thumbnail(image:&RgbaImage)->Result<Vec<u8>,String>{
    let (w,h)=image.dimensions();
    // The Minecraft cape texture is a UV sheet, not a square marketplace
    // thumbnail. For 64x32 textures the visible back is x=12..22,y=1..17.
    let face=if w%64==0 && h%32==0 && w/64==h/32 {
        let scale=w/64;
        imageops::crop_imm(image,12*scale,scale,10*scale,16*scale).to_image()
    }else{
        image.clone()
    };
    // Preview the cape on the *back* of a neutral mannequin, not a flat
    // texture tile. The cape's visible outer back is UV x=12..22 on the
    // standard 64x32 atlas (Minecraft cape cube origin UV [0,0]).
    let resized=imageops::resize(&face,34,43,imageops::FilterType::Nearest);
    let mut canvas=RgbaImage::from_pixel(64,64,Rgba([40,43,48,255]));
    // Mannequin: neck/head, shoulders, arms and feet remain visible behind
    // the cape. Do not embed copyrighted game textures or a fake 'front'.
    fill_thumb_rect(&mut canvas,25,2,39,17,Rgba([129,137,145,255]));
    fill_thumb_rect(&mut canvas,25,2,39,5,Rgba([157,164,171,255]));
    fill_thumb_rect(&mut canvas,10,17,54,37,Rgba([93,104,113,255]));
    fill_thumb_rect(&mut canvas,7,23,15,46,Rgba([119,127,134,255]));
    fill_thumb_rect(&mut canvas,49,23,57,46,Rgba([119,127,134,255]));
    fill_thumb_rect(&mut canvas,19,54,29,62,Rgba([80,88,96,255]));
    fill_thumb_rect(&mut canvas,35,54,45,62,Rgba([80,88,96,255]));
    // Small directional shadow and trim suggest depth, while cape front
    // pattern remains completely determined by the actual outer-back UV.
    fill_thumb_rect(&mut canvas,13,18,50,61,Rgba([29,32,38,255]));
    imageops::overlay(&mut canvas,&resized,15,17);
    let mut out=std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(canvas).write_to(&mut out,ImageFormat::Png)
        .map_err(|e|format!("thumbnail encode failed: {e}"))?;
    Ok(out.into_inner())
}
fn uuid_from(seed:&[u8],kind:&str)->String {
    let mut sha=Sha256::new();
    sha.update(b"Cosmetics/local-capes/v1/");
    sha.update(kind.as_bytes());sha.update([0]);sha.update(seed);
    let digest=sha.finalize();
    let mut raw=[0u8;16];raw.copy_from_slice(&digest[..16]);
    raw[6]=(raw[6]&0x0f)|0x80; // v8 custom deterministic UUID
    raw[8]=(raw[8]&0x3f)|0x80;
    format!("{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        raw[0],raw[1],raw[2],raw[3],raw[4],raw[5],raw[6],raw[7],raw[8],raw[9],raw[10],raw[11],raw[12],raw[13],raw[14],raw[15])
}
fn valid_uuid(id:&str)->bool{crate::persona::is_uuid(id)}
// Bedrock may cache installed Persona resources by pack UUID + version,
// independently of the CDN URL. Generated packs must not retain 1.0.0
// when their texture or generated metadata has changed. Keep the product
// ID and pack UUID stable; only the generated pack's content version changes.
// Native user-provided Persona versions are never replaced.
fn generated_pack_version(name:&str,png:&[u8],author_version:[u32;3])->[u32;3] {
    let mut hash=Sha256::new();
    hash.update(b"Cosmetics/generated-persona-cache-version/v2");
    hash.update(author_version[0].to_le_bytes());
    hash.update(author_version[1].to_le_bytes());
    hash.update(author_version[2].to_le_bytes());
    hash.update(name.as_bytes());
    hash.update([0]);
    hash.update(png);
    let d=hash.finalize();
    let minor=u16::from_le_bytes([d[0],d[1]]) as u32;
    let patch=u16::from_le_bytes([d[2],d[3]]) as u32;
    [author_version[0],minor,if minor==0 && patch==0 {1}else{patch}]
}
fn version_string(v:[u32;3])->String{
    format!("{}.{}.{}",v[0],v[1],v[2])
}
#[derive(Clone,Debug,PartialEq,Eq)]
struct NativeIdentity {uuid:String,version:String}
fn is_binary_bundle(bytes:&[u8])->Result<bool,String>{
    let zip=ZipArchive::new(Cursor::new(bytes))
        .map_err(|e|format!("persona ZIP invalid: {e}"))?;
    let bundled=zip.file_names().flatten().any(|name|name=="ppack0.zip");
    Ok(bundled)
}
fn validate_native(bytes:&[u8])->Result<NativeIdentity,String>{
    // Native Persona data can be a direct pack or a catalog personabinary
    // bundle enclosing ppack0.zip. Validate the inner manifest in both cases.
    let nested=if is_binary_bundle(bytes)?{
        let mut bundle=ZipArchive::new(Cursor::new(bytes))
            .map_err(|e|format!("persona bundle invalid: {e}"))?;
        if bundle.len()>64{return Err("persona bundle has too many members".into());}
        let mut file=bundle.by_name("ppack0.zip").map_err(|e|e.to_string())?;
        if file.size()==0 || file.size()>MAX_NATIVE as u64 {
            return Err("ppack0.zip exceeds Persona native size limit".into());
        }
        let mut inner=Vec::new();
        file.by_ref().take(MAX_NATIVE as u64+1).read_to_end(&mut inner)
            .map_err(|e|e.to_string())?;
        if inner.len()>MAX_NATIVE{return Err("ppack0.zip too large".into());}
        Some(inner)
    }else{None};
    let bytes=nested.as_deref().unwrap_or(bytes);
    let mut zip=ZipArchive::new(Cursor::new(bytes))
        .map_err(|e|format!("native persona.zip invalid: {e}"))?;
    if zip.len()>1024{return Err("native persona.zip has too many members".into());}
    let mut raw=Vec::new();
    {
        let mut file=zip.by_name("manifest.json").map_err(|e|format!("persona manifest missing: {e}"))?;
        if file.size()>MAX_MANIFEST as u64{return Err("persona manifest too large".into());}
        file.by_ref().take(MAX_MANIFEST as u64+1).read_to_end(&mut raw).map_err(|e|e.to_string())?;
    }
    let manifest:Value=serde_json::from_slice(&raw)
        .map_err(|e|format!("persona manifest JSON invalid: {e}"))?;
    let module=manifest.get("modules").and_then(Value::as_array)
        .and_then(|arr|arr.iter().find(|m|
            m.get("type").and_then(Value::as_str)==Some("persona_piece")))
        .ok_or("persona.zip must contain a persona_piece module")?;
    // The persona packIdentity refers to header.uuid, NOT the module UUID.
    // Native Bedrock Persona cache examples have distinct header/module UUIDs.
    if !module.get("uuid").and_then(Value::as_str).is_some_and(valid_uuid) {
        return Err("persona_piece module must have a valid uuid".into());
    }
    let header=manifest.get("header").ok_or("native manifest missing header")?;
    let uuid=header.get("uuid").and_then(Value::as_str)
        .filter(|id|valid_uuid(id))
        .ok_or("native persona header.uuid must be a UUID")?
        .to_ascii_lowercase();
    let version=match header.get("version") {
        Some(Value::Array(parts)) if parts.len()==3 => {
            let parts:Option<Vec<u64>>=parts.iter().map(Value::as_u64).collect();
            let parts=parts.ok_or("native module version must contain numbers")?;
            format!("{}.{}.{}",parts[0],parts[1],parts[2])
        }
        Some(Value::String(s)) if !s.is_empty() && s.bytes().all(|c|c.is_ascii_digit()||c==b'.')
            =>s.clone(),
        _=>return Err("persona_piece module version is missing or invalid".into()),
    };
    Ok(NativeIdentity{uuid,version})
}
fn experimental_persona_png(name:&str,pack_uuid:&str,module_uuid:&str,
    png:&[u8],version:[u32;3])->Result<Vec<u8>,String>{
    crate::persona_pack::generate(name,pack_uuid,module_uuid,png,version)
}

fn name_ok(input:&str)->bool{
    !input.is_empty() && input.chars().count()<=90 && !input.chars().any(char::is_control)
}
// Format v1 multi-cape archives declare a persistent package UUID, a stable
// per-cape key, and a version. No UUID is randomly generated at runtime.
#[derive(Deserialize)]
struct PackManifest {
    format_version: u32,
    id: String,
    name: String,
    #[serde(default)] author: String,
    #[serde(default = "default_pack_version")] version: String,
    capes: Vec<PackCape>,
}
#[derive(Deserialize)]
struct PackCape {
    id: String, // Stable key inside the package; not a random UUID
    name: String,
    #[serde(default)] author: Option<String>,
    #[serde(default)] uuid: Option<String>, // Optional explicit item UUID
    #[serde(default)] texture: Option<String>,
    #[serde(default)] persona_zip: Option<String>,
    #[serde(default)] rarity:Option<String>,
    #[serde(default,rename="startDate",alias="start_date")] start_date:Option<String>,
}
fn default_pack_version()->String {"1.0.0".to_owned()}
const MAX_CAPES_PER_PACK:usize=64;
const MAX_CAPES:usize=512;
struct PreparedCape {
    id:String,
    item:Value,
    catalog:Vec<u8>,
    assets:Vec<(String,LocalAsset)>,
    native:bool,
    dimensions:Option<(u32,u32)>,
    png_color_type:Option<u8>,
    persona_rgba_normalized:bool,
}
fn version_parts(version:&str)->Result<[u32;3],String>{
    let parts:Vec<_>=version.split('.').collect();
    if parts.len()!=3 || parts.iter().any(|x|x.is_empty()
        ||x.len()>5||!x.bytes().all(|b|b.is_ascii_digit())) {
        return Err("version must be major.minor.patch with numeric parts".into());
    }
    let mut result=[0u32;3];
    for (i,part) in parts.into_iter().enumerate(){
        result[i]=part.parse().map_err(|_|"invalid version number")?;
        if result[i]>65535{return Err("version parts must not exceed 65535".into());}
    }
    Ok(result)
}
fn safe_entry(path:&str)->bool{
    !path.is_empty() && path.len()<=192 && !path.starts_with('/')
        && !path.contains('\\') && !path.contains(':')
        && path.split('/').all(|part|
            !part.is_empty() && part!="." && part!=".."
            && part.chars().all(|c|!c.is_control()))
}
/// Match Cosmos' known Persona offer rarity vocabulary; reject invalid user
/// values rather than injecting unexpected values into the Dressing Room.
fn normalized_rarity(value:Option<&str>)->Result<&str,String> {
    let rarity=value.unwrap_or("common");
    if matches!(rarity,"common"|"uncommon"|"rare"|"epic"|"legendary") {
        Ok(rarity)
    } else {Err(format!("unsupported cape rarity: {rarity}"))}
}
/// Only put an author-provided publication date into the offer. A synthetic
/// placeholder date would not be real marketplace metadata.
fn valid_start_date(value:&str)->bool {
    let b=value.as_bytes();
    if b.len()!=20 || b[4]!=b'-'||b[7]!=b'-'||b[10]!=b'T'
        ||b[13]!=b':'||b[16]!=b':'||b[19]!=b'Z'
        ||b.iter().enumerate().any(|(i,c)| ![4,7,10,13,16,19].contains(&i) && !c.is_ascii_digit()){
        return false;
    }
    let num=|a:usize,b:usize|value[a..b].parse::<u32>().unwrap_or(0);
    (2000..=9999).contains(&num(0,4)) && (1..=12).contains(&num(5,7))
        && (1..=31).contains(&num(8,10)) && num(11,13)<24
        && num(14,16)<60 && num(17,19)<60
}
fn stable_key(key:&str)->bool {
    !key.is_empty() && key.len()<=64 && key.bytes()
        .all(|b|b.is_ascii_alphanumeric()||b"_-.".contains(&b))
}
fn optional_file(zip:&mut ZipArchive<File>,name:Option<&str>,max:usize)
    ->Result<Option<Vec<u8>>,String>{
    let Some(name)=name else{return Ok(None)};
    if !safe_entry(name) {return Err(format!("invalid ZIP member path: {name}"));}
    let mut item=zip.by_name(name).map_err(|e|format!("ZIP member {name} missing: {e}"))?;
    if item.size()==0 || item.size()>max as u64{
        return Err(format!("ZIP member {name} has invalid size"));
    }
    let mut bytes=Vec::new();
    item.by_ref().take(max as u64+1).read_to_end(&mut bytes)
        .map_err(|e|format!("ZIP member {name} read failed: {e}"))?;
    if bytes.len()>max{return Err(format!("ZIP member {name} too large"));}
    Ok(Some(bytes))
}
fn optional_file_or_missing(zip:&mut ZipArchive<File>,name:&str,max:usize)
    ->Result<Option<Vec<u8>>,String>{
    if !safe_entry(name){return Err("invalid member path".into());}
    match zip.by_name(name){
        Ok(mut item)=>{
            if item.size()==0 || item.size()>max as u64{
                return Err(format!("ZIP member {name} has invalid size"));
            }
            let mut bytes=Vec::new();
            item.by_ref().take(max as u64+1).read_to_end(&mut bytes)
                .map_err(|e|e.to_string())?;
            if bytes.len()>max{return Err(format!("{name} too large"));}
            Ok(Some(bytes))
        }
        Err(zip::result::ZipError::FileNotFound)=>Ok(None),
        Err(e)=>Err(e.to_string())
    }
}
impl Registry {
    pub fn scan(root:&Path)->Self {
        let mut reg=Self::default();
        let dir=root.join("capes");
        if let Err(e)=fs::create_dir_all(&dir) {
            reg.diagnostics.push(format!("cannot create capes directory: {e}"));
            return reg;
        }
        let mut paths=match fs::read_dir(&dir){
            Ok(iter)=>iter.flatten().map(|x|x.path()).filter(|p|
                p.extension().and_then(|x|x.to_str())
                    .is_some_and(|x|x.eq_ignore_ascii_case("zip"))).collect::<Vec<_>>(),
            Err(e)=>{
                reg.diagnostics.push(format!("cannot scan capes directory: {e}"));
                return reg
            }
        };
        paths.sort();
        reg.discovered_zip=paths.len();
        let skipped=paths.len().saturating_sub(MAX_PACKS);
        if skipped>0{
            reg.diagnostics.push(format!("{skipped} extra ZIP files skipped (limit={MAX_PACKS})"));
        }
        let mut item_ids=HashSet::new();
        let mut asset_ids=HashSet::new();
        let mut total=0usize;
        for path in paths.into_iter().take(MAX_PACKS){
            let label=path.file_name().and_then(|x|x.to_str())
                .unwrap_or("<invalid>").to_owned();
            let result=Self::read_pack(&path);
            match result {
                Ok((name,entries))=>{
                    let items=entries.len();
                    let bytes=entries.iter().flat_map(|entry|entry.assets.iter())
                        .try_fold(0usize,|acc,(_,asset)|acc.checked_add(asset.data.len()))
                        .unwrap_or(usize::MAX);
                    let duplicate=entries.iter().any(|entry|item_ids.contains(&entry.id)
                        ||entry.assets.iter().any(|(path,_)|asset_ids.contains(path)));
                    if duplicate || items>MAX_CAPES.saturating_sub(reg.items.len())
                        ||bytes>MAX_TOTAL_BYTES.saturating_sub(total){
                        reg.rejected+=1;
                        reg.diagnostics.push(format!(
                            "ZIP_REJECTED {label}: duplicate UUID/resource, too many capes or memory limit"));
                        continue;
                    }
                    total+=bytes;
                    for (index,entry) in entries.into_iter().enumerate() {
                        let size=entry.dimensions.map_or("none".to_string(),|(w,h)|
                            format!("{w}x{h} scale={}x",w/64));
                        reg.diagnostics.push(format!(
                            "CAPE_REGISTERED {label} index={} texture={} source_png_color_type={} persona_rgba_normalized={} payload={} game_accepted=UNVERIFIED{}",
                            index+1,size,entry.png_color_type.map_or("none".to_string(),|x|x.to_string()),
                            entry.persona_rgba_normalized,
                            if entry.native{"native_persona"}else{"generated_persona_cfb8"},
                            if entry.native {""} else {" metadata_generated=true game_acceptance_unverified=true"}));
                        item_ids.insert(entry.id.clone());
                        reg.catalogs.insert(entry.id,entry.catalog);
                        for (path,asset) in entry.assets {
                            asset_ids.insert(path.clone());
                            reg.assets.insert(path,asset);
                        }
                        if entry.native{reg.native_payloads+=1}
                        else{reg.generated_persona_payloads+=1}
                        reg.items.push(entry.item);
                    }
                    reg.accepted+=1;
                    reg.diagnostics.push(format!(
                        "ZIP_VALIDATED {label}: {name} capes={items} identity=stable payloads=registered"));
                }
                Err(err)=>{
                    reg.rejected+=1;
                    reg.diagnostics.push(format!("ZIP_REJECTED {label}: {err}"));
                }
            }
        }
        reg
    }

    fn read_pack(path:&Path)->Result<(String,Vec<PreparedCape>),String>{
        let metadata=fs::metadata(path).map_err(|e|e.to_string())?;
        if metadata.len()==0||metadata.len()>MAX_PACK_FILE{
            return Err("pack size exceeds 64MiB".into());
        }
        let mut archive=ZipArchive::new(File::open(path).map_err(|e|e.to_string())?)
            .map_err(|e|format!("invalid outer ZIP: {e}"))?;
        if archive.len()>256{return Err("too many ZIP members".into());}
        let pack_bytes=match archive.by_name("pack.json"){
            Ok(mut file)=>{
                if file.size()>MAX_MANIFEST as u64 {return Err("pack.json too large".into());}
                let mut bytes=Vec::new();
                file.by_ref().take(MAX_MANIFEST as u64+1).read_to_end(&mut bytes)
                    .map_err(|e|e.to_string())?;
                Some(bytes)
            }
            Err(zip::result::ZipError::FileNotFound)=>None,
            Err(e)=>return Err(e.to_string())
        };
        if let Some(bytes)=pack_bytes {
            let pack:PackManifest=serde_json::from_slice(&bytes)
                .map_err(|e|format!("pack.json invalid: {e}"))?;
            if pack.format_version!=1{return Err("unsupported pack.json format_version".into());}
            if !valid_uuid(&pack.id){return Err("pack.json id must be a fixed UUID".into());}
            if !name_ok(&pack.name){return Err("invalid package name".into());}
            if pack.capes.is_empty()||pack.capes.len()>MAX_CAPES_PER_PACK {
                return Err("multi-cape pack must contain 1..64 capes".into());
            }
            let version=version_parts(&pack.version)?;
            let author=if pack.author.is_empty(){"Player"}else{pack.author.as_str()};
            if !name_ok(author){return Err("invalid package author".into());}
            let mut keys=HashSet::new();
            let mut ids=HashSet::new();
            let mut resources=HashSet::new();
            let mut entries=Vec::with_capacity(pack.capes.len());
            for cape in pack.capes {
                if !stable_key(&cape.id) || !keys.insert(cape.id.clone()) {
                    return Err("duplicate or invalid stable cape key".into());
                }
                let item_id=match cape.uuid {
                    Some(id) if valid_uuid(&id)=>id.to_ascii_lowercase(),
                    Some(_)=>return Err("cape uuid must be UUID".into()),
                    None=>uuid_from(
                        format!("{}\0{}",pack.id.to_ascii_lowercase(),cape.id).as_bytes(),"item-v2"),
                };
                if !ids.insert(item_id.clone()){return Err("duplicate cape item UUID".into());}
                let author=cape.author.as_deref().filter(|v|!v.is_empty()).unwrap_or(author);
                let texture=cape.texture.as_deref();
                let persona=cape.persona_zip.as_deref();
                if texture.is_none() && persona.is_none(){
                    return Err(format!("cape {} missing texture or persona_zip",cape.id));
                }
                let png=optional_file(&mut archive,texture,MAX_IMAGE)?;
                let native=optional_file(&mut archive,persona,MAX_NATIVE)?;
                let prepared=Self::prepare_cape(item_id,cape.name,author,png,native,
                    &pack.version,version,cape.rarity.as_deref(),
                    cape.start_date.as_deref())?;
                for (resource,_) in &prepared.assets {
                    if !resources.insert(resource.clone()){
                        return Err("duplicate cape resource UUID".into());
                    }
                }
                entries.push(prepared);
            }
            Ok((pack.name,entries))
        }else{
            // Backward compatible: existing single-cape archives retain their
            // exact previous item/resource identities across an upgrade.
            let cape_json=match archive.by_name("cape.json"){
                Ok(mut f)=>{
                    if f.size()>MAX_MANIFEST as u64{return Err("cape.json too large".into());}
                    let mut bytes=Vec::new();
                    f.by_ref().take(MAX_MANIFEST as u64+1).read_to_end(&mut bytes)
                        .map_err(|e|e.to_string())?;
                    serde_json::from_slice::<CapeManifest>(&bytes)
                        .map_err(|e|format!("invalid cape.json: {e}"))?
                }
                Err(zip::result::ZipError::FileNotFound)=>CapeManifest::default(),
                Err(e)=>return Err(e.to_string())
            };
            let name=if cape_json.name.is_empty(){
                path.file_stem().and_then(|s|s.to_str()).unwrap_or("Local Cape").to_owned()
            }else{cape_json.name};
            let author=if cape_json.author.is_empty(){"Player".to_owned()}else{cape_json.author};
            if !name_ok(&name)||!name_ok(&author){return Err("invalid name or author".into());}
            let legacy_seed=format!("{}\0{}\0{}",
                path.file_stem().and_then(|s|s.to_str()).unwrap_or("cape"),name,author);
            let item_id=match cape_json.id {
                Some(id) if valid_uuid(&id)=>id.to_ascii_lowercase(),
                Some(_)=>return Err("cape.json id must be UUID".into()),
                None=>uuid_from(legacy_seed.as_bytes(),"item")
            };
            let png=optional_file_or_missing(&mut archive,"cape.png",MAX_IMAGE)?;
            let native=optional_file_or_missing(&mut archive,"persona.zip",MAX_NATIVE)?;
            if png.is_none()&&native.is_none(){
                return Err("pack requires cape.png or persona.zip".into());
            }
            let entry=Self::prepare_cape(item_id,name.clone(),&author,png,native,
                "1.0.0",[1,0,0],cape_json.rarity.as_deref(),
                cape_json.start_date.as_deref())?;
            Ok((name,vec![entry]))
        }
    }

    fn prepare_cape(item_id:String,name:String,author:&str,png:Option<Vec<u8>>,
        original:Option<Vec<u8>>,version_text:&str,version:[u32;3],
        rarity:Option<&str>,start_date:Option<&str>)
        ->Result<PreparedCape,String>{
        if !name_ok(&name)||!name_ok(author){return Err("invalid cape name or author".into());}
        let rarity=normalized_rarity(rarity)?;
        if start_date.is_some_and(|date|!valid_start_date(date)) {
            return Err("startDate must be YYYY-MM-DDTHH:MM:SSZ".into());
        }
        let decoded=png.as_ref().map(|data|validate_png(data)).transpose()?;
        let dimensions=decoded.as_ref().map(|img|img.dimensions());
        let native=original.is_some();
        if !native {
            let (w,h)=dimensions.ok_or("PNG-only cape needs a texture")?;
            if w%64!=0 || h%32!=0 || w/64!=h/32 {
                return Err(format!("invalid cape UV dimensions {w}x{h}; expected 64x32, 128x64, 256x128, 512x256 or another 64x32 multiple"));
            }
        }
        // A Persona-only import can have no separate cape.png. Keep every
        // locally generated offer image URL resolvable instead of emitting
        // Cosmos-incompatible empty thumbnail URLs.
        let thumbnail=match decoded.as_ref() {
            Some(image)=>Some(make_thumbnail(image)?),
            None=>Some(make_thumbnail(&RgbaImage::from_pixel(64,32,Rgba([76,88,105,255])))?),
        };
        let module_uuid=uuid_from(item_id.as_bytes(),"resource");
        let generated_pack_uuid=uuid_from(item_id.as_bytes(),"pack");
        let generated_version=if original.is_none(){
            Some(generated_pack_version(&name,png.as_deref().ok_or("missing cape PNG")?,version))
        }else{None};
        let identity=original.as_deref().map(validate_native).transpose()?
            .unwrap_or(NativeIdentity{
                uuid:generated_pack_uuid.clone(),
                version:generated_version.map(version_string).unwrap_or_else(||version_text.to_owned())
            });
        let png_color_type=png.as_deref().and_then(|p|p.get(25)).copied();
        let mut converted_rgba=false;
        let persona=match original {
            // Keep native Persona bytes intact; PNG is only a preview here.
            Some(data)=>if is_binary_bundle(&data)?{data}
                else{crate::persona_pack::bundle_pack(&data)?},
            None=>{
                let source=png.as_deref().ok_or("missing cape PNG")?;
                let decoded=decoded.as_ref().ok_or("missing decoded cape PNG")?;
                let (texture,converted)=persona_rgba_png(source,decoded)?;
                converted_rgba=converted;
                experimental_persona_png(&name,&generated_pack_uuid,
                    &module_uuid,&texture,generated_version.unwrap_or(version))?
            }
        };
        // Key download URLs to generated bytes, in addition to the pack
        // content-version change, to invalidate both HTTP and installed-pack
        // caches without changing the stable product ID or pack UUID.
        // Include the thumbnail in the fingerprint even for native Persona
        // resources where only the sidecar PNG was updated.
        let mut seed=Vec::with_capacity(item_id.len()+version_text.len()+66);
        seed.extend_from_slice(item_id.as_bytes());
        seed.push(0);
        seed.extend_from_slice(version_text.as_bytes());
        seed.push(0);
        seed.extend_from_slice(&Sha256::digest(&persona));
        if let Some(ref image)=thumbnail {
            seed.extend_from_slice(&Sha256::digest(image));
        }
        let resource_id=uuid_from(&seed,"persona-content-v3");
        let url=format!("https://{}/personabinary/{resource_id}.zip",crate::persona::LOCAL_HOST);
        let thumbnail_path=format!("/thumbnails/{resource_id}.png");
        let thumbnail_url=if thumbnail.is_some(){
            format!("https://{}{}",crate::persona::LOCAL_HOST,thumbnail_path)
        }else{String::new()};
        // This is the same offer schema as Cosmos CustomCapes and
        // overrides/capes_items.json. We do not copy Cosmos linksToInfo:
        // its PersonaCreatorPage_master_player_account!CustomCapes endpoint
        // is not enabled in Cosmetics and would create a broken navigation.
        let mut item=json!({
            "id":item_id,"contentType":"PersonaDurable","title":name,
            "creatorName":author,"pieceType":"persona_capes","ownership":"Purchased",
            "price":{"listPrice":0,"realmsInfo":{"inRealmsPlus":false},
                "currencyId":"ecd19d3c-7635-402c-a185-eb11cb6c6946",
                "virtualCurrencyType":"Minecoin"},
            "rarity":rarity,"packType":"Persona",
            "thumbnail":{"tag":"Thumbnail","type":"Thumbnail",
                "url":thumbnail_url,"urlWithResolution":thumbnail_url},
            "images":[{"tag":"Thumbnail","type":"Thumbnail",
                "url":thumbnail_url,"urlWithResolution":thumbnail_url}],
            "packIdentity":[{"type":"persona_piece","uuid":identity.uuid,"version":identity.version}]
        });
        if let Some(start_date)=start_date {
            item["startDate"]=Value::String(start_date.to_string());
        }
        let catalog=json!({
            "code":200,"status":"OK",
            "data":{"Item":{
                "Id":item_id,
                "Contents":[{"Id":resource_id,"Type":"personabinary","Url":url,
                    "MinClientVersion":"1.16.0","MaxClientVersion":"65535.65535.65535"}],
                "DisplayProperties":{"creatorName":author,"purchasable":true,
                    "packIdentity":[{"type":"persona_piece","uuid":identity.uuid,"version":identity.version}]}
            }}
        });
        let mut assets=vec![(format!("/personabinary/{resource_id}.zip"),
            LocalAsset{data:persona,kind:AssetType::PersonaZip})];
        if let Some(bytes)=thumbnail{
            assets.push((thumbnail_path,LocalAsset{data:bytes,kind:AssetType::Png}));
        }
        Ok(PreparedCape{id:item_id,item,catalog:serde_json::to_vec(&catalog)
            .map_err(|e|e.to_string())?,assets,native,dimensions,
            png_color_type,persona_rgba_normalized:converted_rgba})
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    fn dummy_png()->Vec<u8>{
        let mut img=RgbaImage::from_pixel(64,32,Rgba([0,0,0,0]));
        // UV back face should be visible in preview, not flattened gibberish.
        for y in 1..17 {for x in 12..22 { img.put_pixel(x,y,Rgba([220,30,30,255])); }}
        let mut out=Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img).write_to(&mut out,ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test] fn asset_urls_accept_query_without_widening_resource_lookup() {
        assert_eq!(canonical_asset_path("/personabinary/a.zip?version=5"),"/personabinary/a.zip");
        assert_eq!(canonical_asset_path("/thumbnails/b.png?width=800&height=450"),"/thumbnails/b.png");
        assert_eq!(canonical_asset_path("/personabinary/a.zip#section"),"/personabinary/a.zip");
        assert_eq!(canonical_asset_path("/personabinary/a.zip"),"/personabinary/a.zip");
    }
    fn pack_zip(root:&Path,name:&str,manifest:Option<&str>) {
        fs::create_dir_all(root.join("capes")).unwrap();
        let file=File::create(root.join("capes").join(name)).unwrap();
        let mut zip=ZipWriter::new(file);
        let options=SimpleFileOptions::default();
        zip.start_file("cape.png",options).unwrap();
        zip.write_all(&dummy_png()).unwrap();
        if let Some(manifest)=manifest{
            zip.start_file("cape.json",options).unwrap();
            zip.write_all(manifest.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }
    // Build a valid indexed PNG (color type 3) without new dependencies.
    // This exercises the exact failure category in the 1280x640 user ZIP.
    fn png_chunk(out:&mut Vec<u8>,kind:&[u8;4],data:&[u8]){
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let mut crc=0xffff_ffffu32;
        for &byte in kind.iter().chain(data.iter()){
            crc^=byte as u32;
            for _ in 0..8{
                crc=(crc>>1)^(if crc&1==1{0xedb8_8320}else{0});
            }
        }
        out.extend_from_slice(&(!crc).to_be_bytes());
    }
    fn palette_cape_png(scale:u32)->Vec<u8>{
        use flate2::{write::ZlibEncoder,Compression};
        let (w,h)=(64*scale,32*scale);
        let mut rows=Vec::with_capacity(((w+1)*h) as usize);
        for y in 0..h {
            rows.push(0); // no PNG row filter
            for x in 0..w{
                rows.push(u8::from(x>=12*scale && x<22*scale
                    && y>=scale && y<17*scale));
            }
        }
        let mut z=ZlibEncoder::new(Vec::new(),Compression::fast());
        z.write_all(&rows).unwrap();
        let compressed=z.finish().unwrap();
        let mut out=b"\x89PNG\r\n\x1a\n".to_vec();
        let mut ihdr=Vec::with_capacity(13);
        ihdr.extend_from_slice(&w.to_be_bytes());
        ihdr.extend_from_slice(&h.to_be_bytes());
        ihdr.extend_from_slice(&[8,3,0,0,0]); // palette type 3, 8-bit
        png_chunk(&mut out,b"IHDR",&ihdr);
        png_chunk(&mut out,b"PLTE",&[19,25,30, 220,30,40]);
        png_chunk(&mut out,b"IDAT",&compressed);
        png_chunk(&mut out,b"IEND",&[]);
        out
    }
    #[test] fn indexed_1280x640_cape_is_transcoded_to_rgba_for_persona(){
        let original=palette_cape_png(20);
        assert_eq!(original[25],3);
        let decoded=validate_png(&original).unwrap();
        assert_eq!(decoded.dimensions(),(1280,640));
        let (rgba,changed)=persona_rgba_png(&original,&decoded).unwrap();
        assert!(changed);
        assert_eq!(rgba[24],8);
        assert_eq!(rgba[25],6,"Persona texture must be RGBA8, not indexed PNG");
        assert_eq!(validate_png(&rgba).unwrap(),decoded);
        let again=persona_rgba_png(&rgba,&decoded).unwrap();
        assert_eq!(again.0,rgba);
        assert!(!again.1,"RGBA8 input must not be rewritten");
        let id="11111111-2222-4333-8444-555555555555".to_string();
        let cape=Registry::prepare_cape(id.clone(),"HD".to_string(),"Player",
            Some(original.clone()),None,"1.0.0",[1,0,0],None,None).unwrap();
        assert_eq!(cape.dimensions,Some((1280,640)));
        assert_eq!(cape.png_color_type,Some(3));
        assert!(cape.persona_rgba_normalized);
        let module=uuid_from(id.as_bytes(),"resource");
        let pack=uuid_from(id.as_bytes(),"pack");
        let version=generated_pack_version("HD",&original,[1,0,0]);
        let expected=crate::persona_pack::generate("HD",&pack,&module,&rgba,version).unwrap();
        let served=cape.assets.iter().find(|(path,_)|path.starts_with("/personabinary/")).unwrap();
        assert_eq!(served.1.data,expected,"downloaded Persona bundle must contain RGBA-converted texture");
        let thumb=cape.assets.iter().find(|(path,_)|path.starts_with("/thumbnails/")).unwrap();
        let preview=validate_png(&thumb.1.data).unwrap();
        assert_eq!(preview.dimensions(),(64,64));
        assert_eq!(preview.get_pixel(31,33),&Rgba([220,30,40,255]));
        assert_ne!(preview.get_pixel(31,8),&Rgba([220,30,40,255]),
            "neutral mannequin head stays above the back cape");
    }
    #[test] fn cape_preview_uses_outer_back_uv_not_inner_front(){
        let mut image=RgbaImage::from_pixel(64,32,Rgba([0,0,0,0]));
        for y in 1..17 {
            for x in 1..11{image.put_pixel(x,y,Rgba([0,255,0,255]));}
            for x in 12..22{image.put_pixel(x,y,Rgba([255,30,20,255]));}
        }
        let preview=validate_png(&make_thumbnail(&image).unwrap()).unwrap();
        assert_eq!(preview.get_pixel(31,33),&Rgba([255,30,20,255]));
        assert_ne!(preview.get_pixel(31,33),&Rgba([0,255,0,255]));
    }
    #[test] fn one_zip_is_one_cape_without_catalog_files(){
        let root=std::env::temp_dir().join(format!("cosmetics_pack_{}",std::process::id()));
        let _=fs::remove_dir_all(&root);
        pack_zip(&root,"MyCape.zip",Some(r#"{"name":"Red Cape","author":"Example"}"#));
        let reg=Registry::scan(&root);
        assert_eq!(reg.items.len(),1,"{:?}",reg.diagnostics);
        assert_eq!((reg.discovered_zip,reg.accepted,reg.rejected), (1,1,0));
        assert_eq!(reg.generated_persona_payloads,1);
        assert_eq!(reg.items[0]["title"],"Red Cape");
        assert_eq!(reg.items[0]["ownership"],"Purchased");
        let id=reg.items[0]["id"].as_str().unwrap();
        let catalog:Value=serde_json::from_slice(reg.catalogs.get(id).unwrap()).unwrap();
        assert_eq!(catalog["data"]["Item"]["Id"],id);
        let path=catalog["data"]["Item"]["Contents"][0]["Url"].as_str().unwrap()
            .strip_prefix("https://cosmetics.invalid").unwrap();
        assert_eq!(reg.assets.get(path).unwrap().kind,AssetType::PersonaZip);
        let img=reg.assets.values().find(|a|a.kind==AssetType::Png).unwrap();
        let preview=validate_png(&img.data).unwrap();
        assert_eq!(preview.dimensions(),(64,64));
        assert_eq!(preview.get_pixel(32,32),&Rgba([220,30,30,255]));
        let native=&reg.assets.get(path).unwrap().data;
        let mut outer=ZipArchive::new(Cursor::new(native)).unwrap();
        let mut pack=Vec::new();
        outer.by_name("ppack0.zip").unwrap().read_to_end(&mut pack).unwrap();
        let mut zip=ZipArchive::new(Cursor::new(pack)).unwrap();
        assert!(zip.file_names().flatten().any(|name|name.ends_with("_cape.png")));
        let manifest:Value=serde_json::from_reader(zip.by_name("manifest.json").unwrap()).unwrap();
        assert_eq!(manifest["header"]["uuid"],reg.items[0]["packIdentity"][0]["uuid"]);
        assert_ne!(manifest["modules"][0]["uuid"],manifest["header"]["uuid"]);
        let _=fs::remove_dir_all(&root);
    }
    #[test] fn generated_persona_uses_cosmos_bundle_and_encrypted_index() {
        let data=experimental_persona_png("Local Test","11111111-2222-4333-8444-555555555555",
            "66666666-2222-4333-8444-555555555555",&dummy_png(),[1,0,0]).unwrap();
        let mut outer=ZipArchive::new(Cursor::new(data)).unwrap();
        assert_eq!(outer.len(),1);
        let mut nested=Vec::new();
        outer.by_name("ppack0.zip").unwrap().read_to_end(&mut nested).unwrap();
        let mut archive=ZipArchive::new(Cursor::new(nested)).unwrap();
        let manifest:Value=serde_json::from_reader(archive.by_name("manifest.json").unwrap()).unwrap();
        assert_eq!(manifest["format_version"],1);
        assert!(archive.file_names().flatten().any(|path|path.ends_with("_cape.meta.json")));
        let mut index=Vec::new();
        archive.by_name("contents.json").unwrap().read_to_end(&mut index).unwrap();
        assert_eq!(&index[4..8],&2614082044u32.to_le_bytes());
        assert!(archive.by_name("signatures.json").is_ok());
    }
    #[test] fn local_item_metadata_keeps_cosmos_thumbnail_fields() {
        let root=std::env::temp_dir().join(format!("cosmetics_thumbs_{}",std::process::id()));
        let _=fs::remove_dir_all(&root);
        pack_zip(&root,"cape.zip",None);
        let reg=Registry::scan(&root);
        let item=&reg.items[0];
        assert_eq!(item["images"][0]["url"],item["thumbnail"]["url"]);
        assert_eq!(item["images"][0]["urlWithResolution"],item["thumbnail"]["urlWithResolution"]);
        assert_eq!(item["packIdentity"][0]["type"],"persona_piece");
        let _=fs::remove_dir_all(root);
    }
    #[test] fn cosmos_custom_cape_offer_and_catalog_have_matching_identities() {
        let root=std::env::temp_dir().join(format!("cosmetics_cosmos_shape_{}",std::process::id()));
        let _=fs::remove_dir_all(&root);
        pack_zip(&root,"cosmos.zip",Some(r#"{"name":"Sample Cape","author":"Artist",
            "rarity":"legendary","startDate":"2026-10-17T10:20:30Z"}"#));
        let reg=Registry::scan(&root);
        assert_eq!(reg.items.len(),1,"{:?}",reg.diagnostics);
        let item=&reg.items[0];
        for field in ["id","contentType","title","creatorName","thumbnail","price",
            "pieceType","rarity","ownership","packType","packIdentity","images","startDate"] {
            assert!(item.get(field).is_some(),"missing Cosmos offer field {field}");
        }
        assert_eq!(item["contentType"],"PersonaDurable");
        assert_eq!(item["pieceType"],"persona_capes");
        assert_eq!(item["rarity"],"legendary");
        assert_eq!(item["startDate"],"2026-10-17T10:20:30Z");
        assert_eq!(item["thumbnail"]["url"],item["images"][0]["url"]);
        assert!(!item["thumbnail"]["url"].as_str().unwrap().is_empty());
        assert!(item.get("linksToInfo").is_none(),"unhandled Cosmos detail page must not be linked");
        let id=item["id"].as_str().unwrap();
        let catalog:Value=serde_json::from_slice(reg.catalogs.get(id).unwrap()).unwrap();
        let published=&catalog["data"]["Item"];
        assert_eq!(catalog["code"],200);
        assert_eq!(published["Id"],id);
        assert_eq!(published["DisplayProperties"]["packIdentity"],item["packIdentity"]);
        assert_eq!(published["DisplayProperties"]["creatorName"],item["creatorName"]);
        assert_eq!(published["Contents"][0]["Type"],"personabinary");
        assert_eq!(published["Contents"][0]["MinClientVersion"],"1.16.0");
        assert_eq!(published["Contents"][0]["MaxClientVersion"],"65535.65535.65535");
        let url=published["Contents"][0]["Url"].as_str().unwrap();
        assert!(url.starts_with("https://cosmetics.invalid/personabinary/"));
        assert!(reg.assets.contains_key(url.strip_prefix("https://cosmetics.invalid").unwrap()));
        let thumbnail_url=item["thumbnail"]["url"].as_str().unwrap();
        assert!(reg.assets.contains_key(thumbnail_url.strip_prefix("https://cosmetics.invalid").unwrap()));
        let _=fs::remove_dir_all(root);
    }
    #[test] fn invalid_cosmos_offer_metadata_fails_before_registration() {
        let root=std::env::temp_dir().join(format!("cosmetics_bad_offer_{}",std::process::id()));
        let _=fs::remove_dir_all(&root);
        pack_zip(&root,"invalid.zip",Some(r#"{"rarity":"invalid"}"#));
        let reg=Registry::scan(&root);
        assert_eq!(reg.rejected,1);
        assert!(reg.items.is_empty());
        assert!(reg.diagnostics.iter().any(|line|line.contains("unsupported cape rarity")));
        pack_zip(&root,"invalid.zip",Some(r#"{"startDate":"untrusted"}"#));
        let reg=Registry::scan(&root);
        assert_eq!(reg.rejected,1);
        assert!(reg.items.is_empty());
        assert!(reg.diagnostics.iter().any(|line|line.contains("startDate must be")));
        let _=fs::remove_dir_all(root);
    }
    #[test] fn original_native_module_identity_is_used_in_catalog_and_wardrobe() {
        let root=std::env::temp_dir().join(format!("cosmetics_native_{}",std::process::id()));
        let _=fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("capes")).unwrap();
        let uuid="a6c559a1-22b3-44c5-88d6-6ff777888999";
        let module_uuid="12345678-22b3-44c5-88d6-6ff777888999";
        let mut native_zip=ZipWriter::new(Cursor::new(Vec::new()));
        native_zip.start_file("manifest.json",SimpleFileOptions::default()).unwrap();
        native_zip.write_all(format!(
            r#"{{"format_version":1,"header":{{"uuid":"{uuid}","version":[2,3,4]}},"modules":[{{"type":"persona_piece","uuid":"{module_uuid}","version":[9,9,9]}}]}}"#
        ).as_bytes()).unwrap();
        let native=native_zip.finish().unwrap().into_inner();
        let mut outer=ZipWriter::new(File::create(root.join("capes/native.zip")).unwrap());
        outer.start_file("persona.zip",SimpleFileOptions::default()).unwrap();
        outer.write_all(&native).unwrap();
        outer.finish().unwrap();
        let reg=Registry::scan(&root);
        assert_eq!(reg.accepted,1,"{:?}",reg.diagnostics);
        assert_eq!(reg.native_payloads,1);
        assert_eq!(reg.items[0]["packIdentity"][0]["uuid"],uuid);
        assert_eq!(reg.items[0]["packIdentity"][0]["version"],"2.3.4");
        let id=reg.items[0]["id"].as_str().unwrap();
        let product:Value=serde_json::from_slice(reg.catalogs.get(id).unwrap()).unwrap();
        assert_eq!(product["data"]["Item"]["DisplayProperties"]["packIdentity"][0]["uuid"],uuid);
        assert_eq!(product["data"]["Item"]["DisplayProperties"]["packIdentity"][0]["version"],"2.3.4");
        let _=fs::remove_dir_all(&root);
    }
    #[test] fn malformed_png_is_rejected_instead_of_registering_a_false_positive() {
        assert!(validate_png(b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR").is_err());
    }
    #[test] fn unnamed_cape_uses_zip_filename_and_stable_ids() {
        let root=std::env::temp_dir().join(format!("cosmetics_unnamed_{}",std::process::id()));
        let _=fs::remove_dir_all(&root);
        pack_zip(&root,"blue.zip",None);
        let a=Registry::scan(&root);let b=Registry::scan(&root);
        assert_eq!(a.items[0]["id"],b.items[0]["id"]);
        assert_eq!(a.items[0]["title"],"blue");
        let _=fs::remove_dir_all(&root);
    }
    #[test] fn corrupt_zip_is_rejected_without_panic(){
        let root=std::env::temp_dir().join(format!("cosmetics_badpack_{}",std::process::id()));
        let _=fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("capes")).unwrap();
        fs::write(root.join("capes/bad.zip"),b"not a zip").unwrap();
        let reg=Registry::scan(&root);
        assert_eq!(reg.items.len(),0);
        assert_eq!((reg.discovered_zip,reg.accepted,reg.rejected),(1,0,1));
        assert_eq!(reg.diagnostics.len(),1);
        let _=fs::remove_dir_all(&root);
    }
    #[test] fn scan_summary_distinguishes_valid_and_failed_zips() {
        let root=std::env::temp_dir().join(format!("cosmetics_mix_{}",std::process::id()));
        let _=fs::remove_dir_all(&root);
        pack_zip(&root,"working.zip",None);
        fs::write(root.join("capes/broken.zip"),b"not-zip").unwrap();
        let registry=Registry::scan(&root);
        assert_eq!(registry.discovered_zip,2);
        assert_eq!(registry.accepted,1);
        assert_eq!(registry.rejected,1);
        assert_eq!(registry.asset_count(),2);
        assert_eq!(registry.generated_persona_payloads,1);
        assert_eq!(registry.native_payloads,0);
        let _=fs::remove_dir_all(&root);
    }
    fn write_multi(root:&Path,filename:&str,version:&str,duplicate:bool) {
        fs::create_dir_all(root.join("capes")).unwrap();
        let capes=if duplicate {
            json!([
                {"id":"red","name":"Red Cape","texture":"textures/red.png"},
                {"id":"red","name":"Blue Cape","texture":"textures/blue.png"}
            ])
        }else {
            json!([
                {"id":"red","name":"Red Cape","texture":"textures/red.png"},
                {"id":"blue","name":"Blue Cape","texture":"textures/blue.png"}
            ])
        };
        let manifest=json!({
            "format_version":1,
            "id":"11111111-2222-4333-8444-555555555555",
            "name":"My Cape Collection","author":"Test Artist","version":version,
            "capes":capes
        });
        let mut writer=ZipWriter::new(File::create(root.join("capes").join(filename)).unwrap());
        let opts=SimpleFileOptions::default();
        writer.start_file("pack.json",opts).unwrap();
        writer.write_all(&serde_json::to_vec(&manifest).unwrap()).unwrap();
        for path in ["textures/red.png","textures/blue.png"] {
            writer.start_file(path,opts).unwrap();
            writer.write_all(&dummy_png()).unwrap();
        }
        writer.finish().unwrap();
    }
    #[test] fn multi_cape_zip_registers_all_items_and_stable_ids_across_boots_and_rename(){
        let root=std::env::temp_dir().join(format!("cosmetics_multicape_{}",std::process::id()));
        let _=fs::remove_dir_all(&root);
        write_multi(&root,"old-name.zip","1.0.0",false);
        let a=Registry::scan(&root);
        assert_eq!(a.discovered_zip,1);
        assert_eq!(a.accepted,1);
        assert_eq!(a.items.len(),2);
        assert_eq!(a.assets.len(),4);
        assert_eq!(a.generated_persona_payloads,2);
        assert_eq!(a.items[0]["title"],"Red Cape");
        assert_eq!(a.items[1]["title"],"Blue Cape");
        fs::rename(root.join("capes/old-name.zip"),root.join("capes/renamed.zip")).unwrap();
        let b=Registry::scan(&root);
        let c=Registry::scan(&root);
        for i in 0..2 {
            assert_eq!(a.items[i]["id"],b.items[i]["id"]);
            assert_eq!(b.items[i]["id"],c.items[i]["id"]);
            assert_eq!(a.items[i]["packIdentity"],b.items[i]["packIdentity"]);
            let key=a.items[i]["id"].as_str().unwrap();
            assert_eq!(a.catalogs.get(key),b.catalogs.get(key));
        }
        let _=fs::remove_dir_all(root);
    }
    #[test] fn generated_persona_version_is_content_addressed_and_stable() {
        let image=dummy_png();
        let v=generated_pack_version("Example",&image,[1,0,0]);
        assert_ne!(v,[1,0,0],"generated version must not hit obsolete cache");
        assert_eq!(v,generated_pack_version("Example",&image,[1,0,0]));
        assert_ne!(v,generated_pack_version("Renamed",&image,[1,0,0]));
        assert_ne!(v,generated_pack_version("Example",&image,[1,0,1]));
        assert_eq!(version_string(v).split('.').count(),3);
        let id="11111111-2222-4333-8444-555555555555".to_string();
        let make=|data:Vec<u8>|Registry::prepare_cape(
            id.clone(),"Example".to_owned(),"Artist",Some(data),None,
            "1.0.0",[1,0,0],None,None).unwrap();
        let registered=make(image);
        let catalog:Value=serde_json::from_slice(&registered.catalog).unwrap();
        assert_eq!(catalog["data"]["Item"]["DisplayProperties"]["packIdentity"],
            registered.item["packIdentity"]);
        let outer=registered.assets.iter().find(|(path,_)|
            path.starts_with("/personabinary/")).unwrap();
        let mut package=ZipArchive::new(Cursor::new(&outer.1.data)).unwrap();
        let mut bytes=Vec::new();
        package.by_name("ppack0.zip").unwrap().read_to_end(&mut bytes).unwrap();
        let mut inner=ZipArchive::new(Cursor::new(bytes)).unwrap();
        let manifest:Value=serde_json::from_reader(
            inner.by_name("manifest.json").unwrap()).unwrap();
        assert_eq!(manifest["header"]["uuid"],registered.item["packIdentity"][0]["uuid"]);
        let expected=version_string(v);
        assert_eq!(registered.item["packIdentity"][0]["version"],expected);
        assert_eq!(manifest["header"]["version"],json!(v));
        assert_eq!(manifest["modules"][0]["version"],json!(v));
    }
    #[test] fn resource_urls_change_with_generated_persona_content_not_item_identity() {
        let id="11111111-2222-4333-8444-555555555555".to_owned();
        let png=dummy_png();
        let mut modified=Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(RgbaImage::from_pixel(64,32,Rgba([4,5,6,255])))
            .write_to(&mut modified,ImageFormat::Png).unwrap();
        let make=|image:Vec<u8>|Registry::prepare_cape(
            id.clone(),"Local Cape".to_owned(),"Player",Some(image),None,
            "1.0.0",[1,0,0],None,None).unwrap();
        let a=make(png.clone());
        let b=make(modified.into_inner());
        let again=make(png);
        assert_eq!(a.id,b.id);
        assert_eq!(a.item["packIdentity"][0]["uuid"],b.item["packIdentity"][0]["uuid"]);
        assert_ne!(a.item["packIdentity"][0]["version"],b.item["packIdentity"][0]["version"],
            "changed PNG must invalidate the Persona package version");
        assert_eq!(a.item["packIdentity"],again.item["packIdentity"]);
        let cat_a:Value=serde_json::from_slice(&a.catalog).unwrap();
        let cat_b:Value=serde_json::from_slice(&b.catalog).unwrap();
        let cat_again:Value=serde_json::from_slice(&again.catalog).unwrap();
        assert_ne!(cat_a["data"]["Item"]["Contents"][0]["Id"],
            cat_b["data"]["Item"]["Contents"][0]["Id"]);
        assert_ne!(cat_a["data"]["Item"]["Contents"][0]["Url"],
            cat_b["data"]["Item"]["Contents"][0]["Url"]);
        assert_ne!(a.item["thumbnail"]["url"],b.item["thumbnail"]["url"]);
        assert_eq!(cat_a["data"]["Item"]["Contents"][0]["Url"],
            cat_again["data"]["Item"]["Contents"][0]["Url"]);
        assert_eq!(a.item["thumbnail"]["url"],again.item["thumbnail"]["url"]);
    }
    #[test] fn explicit_pack_version_changes_asset_urls_not_item_identity(){
        let root=std::env::temp_dir().join(format!("cosmetics_packversion_{}",std::process::id()));
        let _=fs::remove_dir_all(&root);
        write_multi(&root,"collection.zip","1.0.0",false);
        let a=Registry::scan(&root);
        write_multi(&root,"collection.zip","1.0.1",false);
        let b=Registry::scan(&root);
        for i in 0..2{
            assert_eq!(a.items[i]["id"],b.items[i]["id"]);
            assert_eq!(a.items[i]["packIdentity"][0]["uuid"],b.items[i]["packIdentity"][0]["uuid"]);
            assert_ne!(a.items[i]["packIdentity"][0]["version"],
                b.items[i]["packIdentity"][0]["version"],
                "author pack version changes must invalidate installed Persona");
            let v=a.items[i]["packIdentity"][0]["version"].as_str().unwrap();
            assert!(v.starts_with("1."));
            let updated=b.items[i]["packIdentity"][0]["version"].as_str().unwrap();
            assert!(updated.starts_with("1."));
            let id=a.items[i]["id"].as_str().unwrap();
            let first:Value=serde_json::from_slice(a.catalogs.get(id).unwrap()).unwrap();
            let next:Value=serde_json::from_slice(b.catalogs.get(id).unwrap()).unwrap();
            assert_ne!(first["data"]["Item"]["Contents"][0]["Url"],
                next["data"]["Item"]["Contents"][0]["Url"]);
        }
        let _=fs::remove_dir_all(root);
    }
    #[test] fn invalid_multi_cape_pack_fails_atomically_without_partial_registration(){
        let root=std::env::temp_dir().join(format!("cosmetics_duplicate_{}",std::process::id()));
        let _=fs::remove_dir_all(&root);
        write_multi(&root,"duplicate.zip","1.0.0",true);
        let reg=Registry::scan(&root);
        assert_eq!(reg.discovered_zip,1);
        assert_eq!(reg.rejected,1);
        assert!(reg.items.is_empty());
        assert!(reg.assets.is_empty());
        assert!(reg.catalogs.is_empty());
        let _=fs::remove_dir_all(root);
    }
    #[test] fn multi_cape_pack_rejects_traversal_and_invalid_versions(){
        assert!(!safe_entry("../textures/red.png"));
        assert!(!safe_entry("textures\\red.png"));
        assert!(!safe_entry("/outside.png"));
        assert!(safe_entry("textures/red.png"));
        assert!(safe_entry("textures/我的披风.png"));
        assert!(safe_entry("textures/Red Cape.png"));
        assert_eq!(version_parts("1.2.3").unwrap(),[1,2,3]);
        assert!(version_parts("1.2").is_err());
        assert!(version_parts("1.2.999999").is_err());
    }
    #[test] fn high_resolution_png_uv_is_preserved_in_generated_persona_zip(){
        for (scale,w,h) in [(2,128,64),(4,256,128),(8,512,256),(20,1280,640)]{
            let root=std::env::temp_dir().join(format!(
                "cosmetics_hires_{}_{}",std::process::id(),scale));
            let _=fs::remove_dir_all(&root);
            fs::create_dir_all(root.join("capes")).unwrap();
            let mut image=RgbaImage::from_pixel(w,h,Rgba([0,0,0,0]));
            for y in scale..17*scale{
                for x in 12*scale..22*scale {image.put_pixel(x,y,Rgba([200,50,80,255]));}
            }
            let mut encoded=Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(image)
                .write_to(&mut encoded,ImageFormat::Png).unwrap();
            let mut outer=ZipWriter::new(File::create(root.join("capes/hires.zip")).unwrap());
            outer.start_file("cape.png",SimpleFileOptions::default()).unwrap();
            outer.write_all(encoded.get_ref()).unwrap();
            outer.finish().unwrap();
            let reg=Registry::scan(&root);
            assert_eq!(reg.accepted,1,"{:?}",reg.diagnostics);
            assert_eq!(reg.generated_persona_payloads,1);
            let data=reg.assets.values().find(|v|v.kind==AssetType::PersonaZip).unwrap();
            let mut outer=ZipArchive::new(Cursor::new(&data.data)).unwrap();
            let mut nested=Vec::new();
            outer.by_name("ppack0.zip").unwrap().read_to_end(&mut nested).unwrap();
            let mut inner=ZipArchive::new(Cursor::new(nested)).unwrap();
            let texture=inner.file_names().flatten().find(|path|path.ends_with("_cape.png"))
                .unwrap().to_string();
            let mut encrypted=Vec::new();
            inner.by_name(&texture).unwrap().read_to_end(&mut encrypted).unwrap();
            assert_ne!(encrypted.as_slice(),encoded.get_ref().as_slice(),"PNG must be encrypted");
            assert_eq!(encrypted.len()%16,0);
            let decoded=validate_png(encoded.get_ref()).unwrap();
            assert_eq!(decoded.dimensions(),(w,h));
            assert!(reg.diagnostics.iter().any(|line|
                line.contains(&format!("texture={w}x{h}"))));
            let _=fs::remove_dir_all(root);
        }
    }
    #[test] fn non_cape_aspect_ratio_is_rejected_with_diagnostic(){
        let root=std::env::temp_dir().join(format!("cosmetics_wrong_uv_{}",std::process::id()));
        let _=fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("capes")).unwrap();
        let image=RgbaImage::from_pixel(128,128,Rgba([255,20,20,255]));
        let mut bytes=Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut bytes,ImageFormat::Png).unwrap();
        let mut zip=ZipWriter::new(File::create(root.join("capes/wrong.zip")).unwrap());
        zip.start_file("cape.png",SimpleFileOptions::default()).unwrap();
        zip.write_all(bytes.get_ref()).unwrap();
        zip.finish().unwrap();
        let reg=Registry::scan(&root);
        assert_eq!(reg.rejected,1);
        assert_eq!(reg.items.len(),0);
        assert!(reg.diagnostics.iter().any(|x|x.contains("invalid cape UV dimensions 128x128")));
        let _=fs::remove_dir_all(root);
    }
    #[test] fn cape_png_requires_png_signature(){
        assert!(validate_png(b"GIF89a").is_err());
    }
}
