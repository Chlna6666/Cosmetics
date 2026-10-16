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

#[derive(Debug,Default,Deserialize)]
struct CapeManifest {
    #[serde(default)] name:String,
    #[serde(default)] author:String,
    #[serde(default)] id:Option<String>,
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
    pub plaintext_experimental_payloads:usize,
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
pub fn asset(path:&str)->Option<(&'static [u8],AssetType)> {
    let a=REGISTRY.get()?.assets.get(path)?;
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
    let resized=imageops::resize(&face,40,56,imageops::FilterType::Nearest);
    let mut canvas=RgbaImage::from_pixel(64,64,Rgba([40,43,48,255]));
    imageops::overlay(&mut canvas,&resized,12,4);
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
#[derive(Clone,Debug,PartialEq,Eq)]
struct NativeIdentity {uuid:String,version:String}
fn validate_native(bytes:&[u8])->Result<NativeIdentity,String>{
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
fn experimental_persona_png(name:&str,pack_uuid:&str,module_uuid:&str,png:&[u8])->Result<Vec<u8>,String>{
    // This is a plaintext, *experimental* Persona resource payload, not a
    // verified Minecraft Marketplace personabinary file or signed package.
    let manifest=json!({
        "format_version":2,
        "header":{"name":name,"description":"Cosmetics local plaintext cape experiment",
                  "uuid":pack_uuid,"version":[1,0,0],"min_engine_version":[1,16,0]},
        "modules":[{"type":"persona_piece","uuid":module_uuid,"version":[1,0,0]}]
    });
    let files=json!({"content":[{"path":"manifest.json"},{"path":"textures/cape.png"},{"path":"contents.json"}]});
    let mut writer=ZipWriter::new(Cursor::new(Vec::new()));
    let options=SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (path,data) in [
        ("manifest.json",serde_json::to_vec(&manifest).map_err(|e|e.to_string())?),
        ("contents.json",serde_json::to_vec(&files).map_err(|e|e.to_string())?),
        ("textures/cape.png",png.to_vec())
    ] {
        writer.start_file(path,options).map_err(|e|e.to_string())?;
        writer.write_all(&data).map_err(|e|e.to_string())?;
    }
    Ok(writer.finish().map_err(|e|e.to_string())?.into_inner())
}
fn name_ok(input:&str)->bool{
    !input.is_empty() && input.chars().count()<=90 && !input.chars().any(char::is_control)
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
                p.extension().and_then(|x|x.to_str()).is_some_and(|x|x.eq_ignore_ascii_case("zip"))).collect::<Vec<_>>(),
            Err(e)=>{
                reg.diagnostics.push(format!("cannot scan capes directory: {e}"));return reg
            }
        };
        paths.sort();
        reg.discovered_zip=paths.len();
        let skipped=paths.len().saturating_sub(MAX_PACKS);
        if skipped>0 {
            reg.diagnostics.push(format!("{skipped} extra ZIP files skipped (limit={MAX_PACKS})"));
        }
        let mut reserved=HashSet::new();
        let mut total=0usize;
        for path in paths.into_iter().take(MAX_PACKS){
            let label=path.file_name().and_then(|x|x.to_str()).unwrap_or("<invalid>").to_owned();
            match Self::read_one(&path,&mut reg,&mut reserved,&mut total){
                Ok((name,native))=>{
                    reg.accepted+=1;
                    if native {reg.native_payloads+=1;}else{reg.plaintext_experimental_payloads+=1;}
                    reg.diagnostics.push(format!("ZIP_VALIDATED {label}: {name} payload={}",
                        if native {"native_persona"}else{"experimental_plaintext_png"}));
                }
                Err(err)=>{
                    reg.rejected+=1;
                    reg.diagnostics.push(format!("ZIP_REJECTED {label}: {err}"));
                }
            }
        }
        reg
    }
    fn read_one(path:&Path,reg:&mut Registry,reserved:&mut HashSet<String>,total:&mut usize)->Result<(String,bool),String>{
        let metadata=fs::metadata(path).map_err(|e|e.to_string())?;
        if metadata.len()==0||metadata.len()>MAX_PACK_FILE{return Err("pack size exceeds 64MiB".into());}
        let mut zip=ZipArchive::new(File::open(path).map_err(|e|e.to_string())?)
            .map_err(|e|format!("invalid outer ZIP: {e}"))?;
        if zip.len()>64{return Err("too many ZIP entries".into());}
        let cape_json=match zip.by_name("cape.json"){
            Ok(mut file)=>{
                if file.size()>MAX_MANIFEST as u64{return Err("cape.json too large".into());}
                let mut bytes=Vec::new();
                file.by_ref().take(MAX_MANIFEST as u64+1).read_to_end(&mut bytes)
                    .map_err(|e|e.to_string())?;
                serde_json::from_slice::<CapeManifest>(&bytes)
                    .map_err(|e|format!("invalid cape.json: {e}"))?
            }
            Err(zip::result::ZipError::FileNotFound)=>CapeManifest::default(),
            Err(e)=>return Err(e.to_string())
        };
        let name=if cape_json.name.is_empty(){
            path.file_stem().and_then(|s|s.to_str()).unwrap_or("Local Cape").to_string()
        }else{cape_json.name};
        let author=if cape_json.author.is_empty(){"Player".to_string()}else{cape_json.author};
        if !name_ok(&name)||!name_ok(&author){return Err("invalid name or author".into());}
        let seed=format!("{}\0{}\0{}",path.file_stem().and_then(|s|s.to_str()).unwrap_or("cape"),name,author);
        let id=match cape_json.id{
            Some(id) if valid_uuid(&id)=>id.to_ascii_lowercase(),
            Some(_)=>return Err("cape.json id must be a UUID".into()),
            None=>uuid_from(seed.as_bytes(),"item")
        };
        let resource_id=uuid_from(id.as_bytes(),"resource");
        let generated_pack_uuid=uuid_from(id.as_bytes(),"pack");
        if !reserved.insert(id.clone()){return Err("duplicate local cape ID".into());}
        let png=match zip.by_name("cape.png"){
            Ok(mut file)=>{
                if file.size()>MAX_IMAGE as u64{return Err("cape.png exceeds 8MiB".into());}
                let mut data=Vec::new();
                file.by_ref().take(MAX_IMAGE as u64+1).read_to_end(&mut data).map_err(|e|e.to_string())?;
                let img=validate_png(&data)?;
                Some((data,img))
            }
            Err(zip::result::ZipError::FileNotFound)=>None,
            Err(e)=>return Err(e.to_string()),
        };
        let original=match zip.by_name("persona.zip"){
            Ok(mut file)=>{
                if file.size()>MAX_NATIVE as u64{return Err("persona.zip exceeds 64MiB".into());}
                let mut data=Vec::new();
                file.by_ref().take(MAX_NATIVE as u64+1).read_to_end(&mut data).map_err(|e|e.to_string())?;
                let native=validate_native(&data)?;
                Some((data,native))
            }
            Err(zip::result::ZipError::FileNotFound)=>None,
            Err(e)=>return Err(e.to_string()),
        };
        if png.is_none()&&original.is_none(){return Err("pack requires cape.png or persona.zip".into());}
        let is_native=original.is_some();
        let identity=original.as_ref().map(|(_,identity)|identity.clone())
            .unwrap_or(NativeIdentity{uuid:generated_pack_uuid.clone(),version:"1.0.0".to_string()});
        let persona=match original{
            Some((payload,_))=>payload,
            None=>experimental_persona_png(&name,&generated_pack_uuid,&resource_id,&png.as_ref().unwrap().0)?
        };
        let thumb=png.map(|(_png,img)| {
            let path=format!("/thumbnails/{resource_id}.png");
            make_thumbnail(&img).map(|bytes|(path,bytes))
        }).transpose()?;
        let required=persona.len()+thumb.as_ref().map_or(0,|(_,bytes)|bytes.len());
        if required>MAX_TOTAL_BYTES.saturating_sub(*total){
            return Err("total in-memory local cape budget exceeded".into());
        }
        *total+=required;
        let url=format!("https://{}/personabinary/{resource_id}.zip",crate::persona::LOCAL_HOST);
        let thumbnail=thumb.as_ref().map(|(path,_)|
            format!("https://{}{}",crate::persona::LOCAL_HOST,path))
            .unwrap_or_default();
        let item=json!({
            "id":id,
            "contentType":"PersonaDurable",
            "title":name,
            "creatorName":author,
            "pieceType":"persona_capes",
            "ownership":"Purchased",
            "price":{"listPrice":0,"realmsInfo":{"inRealmsPlus":false},
                "currencyId":"ecd19d3c-7635-402c-a185-eb11cb6c6946",
                "virtualCurrencyType":"Minecoin"},
            "rarity":"common",
            "packType":"Persona",
            "thumbnail":{"tag":"Thumbnail","type":"Thumbnail","url":thumbnail,"urlWithResolution":thumbnail},
            "packIdentity":[{"type":"persona_piece","uuid":identity.uuid,"version":identity.version}]
        });
        let catalog=json!({
            "code":200,"status":"OK",
            "data":{"Item":{
                "Id":id,
                "Contents":[{"Id":resource_id,"Type":"personabinary","Url":url,
                    "MinClientVersion":"1.16.0","MaxClientVersion":"65535.65535.65535"}],
                "DisplayProperties":{"creatorName":author,"purchasable":true,
                    "packIdentity":[{"type":"persona_piece","uuid":identity.uuid,"version":identity.version}]}
            }}
        });
        // Product and native module identities must agree. This is not
        // proof the game will accept a synthetic unsigned Persona package.
        // Commit all outputs together only after complete validation.
        let catalog_bytes=serde_json::to_vec(&catalog).map_err(|e|e.to_string())?;
        reg.catalogs.insert(id,catalog_bytes);
        reg.assets.insert(format!("/personabinary/{resource_id}.zip"),
            LocalAsset{data:persona,kind:AssetType::PersonaZip});
        if let Some((path,bytes))=thumb{
            reg.assets.insert(path,LocalAsset{data:bytes,kind:AssetType::Png});
        }
        reg.items.push(item);
        Ok((name,is_native))
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
    #[test] fn one_zip_is_one_cape_without_catalog_files(){
        let root=std::env::temp_dir().join(format!("cosmetics_pack_{}",std::process::id()));
        let _=fs::remove_dir_all(&root);
        pack_zip(&root,"MyCape.zip",Some(r#"{"name":"Red Cape","author":"Example"}"#));
        let reg=Registry::scan(&root);
        assert_eq!(reg.items.len(),1,"{:?}",reg.diagnostics);
        assert_eq!((reg.discovered_zip,reg.accepted,reg.rejected), (1,1,0));
        assert_eq!(reg.plaintext_experimental_payloads,1);
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
        let mut zip=ZipArchive::new(Cursor::new(native)).unwrap();
        assert!(zip.by_name("textures/cape.png").is_ok());
        let manifest:Value=serde_json::from_reader(zip.by_name("manifest.json").unwrap()).unwrap();
        assert_eq!(manifest["header"]["uuid"],reg.items[0]["packIdentity"][0]["uuid"]);
        assert_ne!(manifest["modules"][0]["uuid"],manifest["header"]["uuid"]);
        let _=fs::remove_dir_all(&root);
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
        assert_eq!(registry.plaintext_experimental_payloads,1);
        assert_eq!(registry.native_payloads,0);
        let _=fs::remove_dir_all(&root);
    }
    #[test] fn cape_png_requires_png_signature(){
        assert!(validate_png(b"GIF89a").is_err());
    }
}
