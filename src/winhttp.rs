//! WinHTTP synchronous response overlay. The DLL is standalone and has no
//! dependency on BLoader or Minecraft's internal player/skin APIs.
//! Requests with unknown handles, unsupported encodings, and async WinHTTP
//! sessions always pass through. Resource ZIP URLs are not intercepted.
use crate::response::{self, Config, Mode, Rule};
use std::collections::HashMap;
use std::ffi::c_void;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Mutex, OnceLock};

type H = *mut c_void;
type B = i32;
const ASYNC_FLAG: u32 = 0x1000_0000;
const QUERY_NUMBER: u32 = 0x2000_0000;
const QUERY_CONTENT_TYPE: u32 = 1;
const QUERY_CONTENT_LENGTH: u32 = 5;
const QUERY_STATUS_CODE: u32 = 19;
const QUERY_CONTENT_ENCODING: u32 = 29;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const REQUEST_BODY_MAX: usize = 128 * 1024;
const READ_CHUNK: usize = 16 * 1024;

#[link(name = "kernel32")]
extern "system" {
    fn CreateThread(attr: H, stack: usize, entry: Option<unsafe extern "system" fn(H) -> u32>,
                    param: H, flags: u32, tid: *mut u32) -> H;
    fn CloseHandle(handle: H) -> B;
    fn LoadLibraryW(name: *const u16) -> H;
    fn GetProcAddress(module: H, name: *const u8) -> H;
    fn GetModuleFileNameW(module: H, path: *mut u16, size: u32) -> u32;
    fn DisableThreadLibraryCalls(module: H) -> B;
    fn SetLastError(code: u32);
}

type OpenFn = unsafe extern "system" fn(*const u16,u32,*const u16,*const u16,u32)->H;
type ConnectFn = unsafe extern "system" fn(H,*const u16,u16,u32)->H;
type OpenRequestFn = unsafe extern "system" fn(H,*const u16,*const u16,*const u16,*const u16,*const *const u16,u32)->H;
type SendFn = unsafe extern "system" fn(H,*const u16,u32,H,u32,u32,usize)->B;
type WriteFn = unsafe extern "system" fn(H,*const c_void,u32,*mut u32)->B;
type ReceiveFn = unsafe extern "system" fn(H,*mut c_void)->B;
type ReadFn = unsafe extern "system" fn(H,H,u32,*mut u32)->B;
type AvailableFn = unsafe extern "system" fn(H,*mut u32)->B;
type HeadersFn = unsafe extern "system" fn(H,u32,*const u16,H,*mut u32,*mut u32)->B;
type CloseFn = unsafe extern "system" fn(H)->B;

static OPEN: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());
static CONNECT: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());
static OPEN_REQUEST: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());
static SEND: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());
static WRITE: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());
static RECEIVE: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());
static READ: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());
static AVAILABLE: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());
static HEADERS: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());
static CLOSE: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());

macro_rules! original {
    ($slot:ident, $type:ty) => {{
        // Each trampoline is stored before the hook group is enabled.
        unsafe { std::mem::transmute::<H, $type>($slot.load(Ordering::Acquire)) }
    }};
}

struct Runtime {
    cfg: Config,
    root: PathBuf,
    log: Mutex<File>,
}
static RUNTIME: OnceLock<Runtime> = OnceLock::new();
fn logger(message: &str) {
    if let Some(r) = RUNTIME.get() {
        let mut out = r.log.lock().unwrap_or_else(|e| e.into_inner());
        // No full URLs, request bodies, response bodies, auth headers, or account IDs.
        let _ = writeln!(out, "[Cosmetics] {message}");
        let _ = out.flush();
    }
}

#[derive(Default)]
struct Tables {
    sessions: HashMap<usize, bool>,
    connections: HashMap<usize, (String, bool)>,
    requests: HashMap<usize, Request>,
}
struct Request {
    host: String,
    path: String,
    method: String,
    async_session: bool,
    request_body: Vec<u8>,
    replay: Option<Replay>,
}
struct Replay {
    bytes: Vec<u8>,
    position: usize,
    complete: bool,
    replaced: bool,
    catalog: bool,
}
static TABLES: OnceLock<Mutex<Tables>> = OnceLock::new();
fn tables() -> &'static Mutex<Tables> {
    TABLES.get_or_init(|| Mutex::new(Tables::default()))
}
fn read_wide(p: *const u16) -> String {
    if p.is_null() { return String::new(); }
    unsafe {
        let mut n = 0usize;
        while n < 2048 && *p.add(n) != 0 { n += 1; }
        String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
    }
}
fn state_key(handle: H) -> usize { handle as usize }

pub unsafe fn spawn_worker(module: H) {
    let _ = DisableThreadLibraryCalls(module);
    let h = CreateThread(ptr::null_mut(), 0, Some(worker), module, 0, ptr::null_mut());
    if !h.is_null() { let _ = CloseHandle(h); }
}
unsafe extern "system" fn worker(module: H) -> u32 {
    // Never run filesystem, std::thread, networking or MinHook initialization in DllMain.
    let mut path = vec![0u16; 32768];
    let count = GetModuleFileNameW(module, path.as_mut_ptr(), path.len() as u32);
    if count == 0 || count as usize >= path.len() { return 1; }
    let dll = PathBuf::from(String::from_utf16_lossy(&path[..count as usize]));
    let root = match dll.parent() { Some(p) => p.to_path_buf(), None => return 2 };
    let cfg_bytes = fs::read(root.join("cosmetics.json"))
        .unwrap_or_else(|_| include_bytes!("../cosmetics.json").to_vec());
    let cfg: Config = match serde_json::from_slice(&cfg_bytes) { Ok(v) => v, Err(_) => return 4 };
    let _ = fs::create_dir_all(root.join("logs"));
    let log = match OpenOptions::new().create(true).append(true)
        .open(root.join("logs").join("cosmetics.log")) { Ok(f) => f, Err(_) => return 5 };
    let _ = RUNTIME.set(Runtime { cfg, root, log });
    logger("initializing independent WinHTTP module; fail-open is enabled");
    if let Err(e) = install() {
        logger(&format!("hook install failed; game remains unmodified: {e}"));
        return 6;
    }
    logger("WinHTTP hooks enabled (synchronous requests only)");
    0
}

unsafe fn install() -> Result<(), String> {
    let dll_name: Vec<u16> = "winhttp.dll".encode_utf16().chain(Some(0)).collect();
    let module = LoadLibraryW(dll_name.as_ptr());
    if module.is_null() { return Err("winhttp.dll not available".into()); }
    if minhook_sys::MH_Initialize() != minhook_sys::MH_OK {
        return Err("MH_Initialize failed".into());
    }
    let defs: &[(&[u8], H, &AtomicPtr<c_void>)] = &[
        (b"WinHttpOpen\0", on_open as H, &OPEN),
        (b"WinHttpConnect\0", on_connect as H, &CONNECT),
        (b"WinHttpOpenRequest\0", on_open_request as H, &OPEN_REQUEST),
        (b"WinHttpSendRequest\0", on_send as H, &SEND),
        (b"WinHttpWriteData\0", on_write as H, &WRITE),
        (b"WinHttpReceiveResponse\0", on_receive as H, &RECEIVE),
        (b"WinHttpReadData\0", on_read as H, &READ),
        (b"WinHttpQueryDataAvailable\0", on_available as H, &AVAILABLE),
        (b"WinHttpQueryHeaders\0", on_headers as H, &HEADERS),
        (b"WinHttpCloseHandle\0", on_close as H, &CLOSE),
    ];
    let mut installed: Vec<H> = Vec::new();
    for (symbol, detour, slot) in defs {
        let addr = GetProcAddress(module, symbol.as_ptr());
        if addr.is_null() {
            for p in installed { let _ = minhook_sys::MH_RemoveHook(p); }
            let _ = minhook_sys::MH_Uninitialize();
            return Err(format!("WinHTTP export missing: {:?}", symbol));
        }
        let mut trampoline: H = ptr::null_mut();
        if minhook_sys::MH_CreateHook(addr, *detour, &mut trampoline) != minhook_sys::MH_OK {
            for p in installed { let _ = minhook_sys::MH_RemoveHook(p); }
            let _ = minhook_sys::MH_Uninitialize();
            return Err(format!("MH_CreateHook failed for {:?}", symbol));
        }
        slot.store(trampoline, Ordering::Release);
        installed.push(addr);
    }
    for addr in &installed {
        if minhook_sys::MH_QueueEnableHook(*addr) != minhook_sys::MH_OK {
            for p in installed { let _ = minhook_sys::MH_RemoveHook(p); }
            let _ = minhook_sys::MH_Uninitialize();
            return Err("MH_QueueEnableHook failed".into());
        }
    }
    if minhook_sys::MH_ApplyQueued() != minhook_sys::MH_OK {
        for p in installed { let _ = minhook_sys::MH_DisableHook(p); let _ = minhook_sys::MH_RemoveHook(p); }
        let _ = minhook_sys::MH_Uninitialize();
        return Err("MH_ApplyQueued failed".into());
    }
    Ok(())
}

unsafe extern "system" fn on_open(agent:*const u16,access:u32,proxy:*const u16,bypass:*const u16,flags:u32)->H {
    let real = original!(OPEN, OpenFn);
    let h = real(agent,access,proxy,bypass,flags);
    if !h.is_null() {
        tables().lock().unwrap_or_else(|e|e.into_inner()).sessions
            .insert(state_key(h), flags & ASYNC_FLAG != 0);
    }
    h
}
unsafe extern "system" fn on_connect(session:H,server:*const u16,port:u16,reserved:u32)->H {
    let real = original!(CONNECT, ConnectFn);
    let h = real(session,server,port,reserved);
    if !h.is_null() {
        let mut state = tables().lock().unwrap_or_else(|e|e.into_inner());
        let is_async = state.sessions.get(&state_key(session)).copied().unwrap_or(true);
        state.connections.insert(state_key(h),(read_wide(server),is_async));
    }
    h
}
unsafe extern "system" fn on_open_request(
    connection:H,verb:*const u16,path:*const u16,version:*const u16,referer:*const u16,
    accepts:*const *const u16,flags:u32
)->H {
    let real = original!(OPEN_REQUEST, OpenRequestFn);
    let h = real(connection,verb,path,version,referer,accepts,flags);
    if !h.is_null() {
        let mut state = tables().lock().unwrap_or_else(|e|e.into_inner());
        let (host,async_session) = state.connections.get(&state_key(connection))
            .cloned().unwrap_or_else(||(String::new(),true));
        let req = Request {
            host, path:read_wide(path), method:if verb.is_null() { "GET".into() } else { read_wide(verb) },
            async_session, request_body:Vec::new(), replay:None,
        };
        state.requests.insert(state_key(h),req);
    }
    h
}
unsafe extern "system" fn on_send(
    h:H,headers:*const u16,header_len:u32,optional:H,optional_len:u32,total_len:u32,ctx:usize
)->B {
    if !optional.is_null() && (optional_len as usize) <= REQUEST_BODY_MAX {
        let bytes = std::slice::from_raw_parts(optional as *const u8,optional_len as usize);
        if let Some(req) = tables().lock().unwrap_or_else(|e|e.into_inner())
            .requests.get_mut(&state_key(h)) {
            req.request_body.extend_from_slice(bytes);
        }
    }
    original!(SEND, SendFn)(h,headers,header_len,optional,optional_len,total_len,ctx)
}
unsafe extern "system" fn on_write(h:H,data:*const c_void,len:u32,written:*mut u32)->B {
    let result = original!(WRITE, WriteFn)(h,data,len,written);
    if result != 0 && !data.is_null() {
        let transferred = if written.is_null() { 0 } else { *written as usize };
        let mut state = tables().lock().unwrap_or_else(|e|e.into_inner());
        if let Some(req) = state.requests.get_mut(&state_key(h)) {
            if transferred <= REQUEST_BODY_MAX.saturating_sub(req.request_body.len()) {
                req.request_body.extend_from_slice(std::slice::from_raw_parts(data as *const u8,transferred));
            }
        }
    }
    result
}

unsafe fn query_number(h:H,code:u32)->Option<u32> {
    let mut n:u32=0;
    let mut bytes:u32=4;
    let ok=original!(HEADERS, HeadersFn)(
        h,code|QUERY_NUMBER,ptr::null(),&mut n as *mut u32 as H,&mut bytes,ptr::null_mut()
    );
    (ok!=0).then_some(n)
}
unsafe fn query_string(h:H,code:u32)->Option<String> {
    let mut wide=[0u16;128]; let mut bytes=(wide.len()*2) as u32;
    let ok=original!(HEADERS, HeadersFn)(
        h,code,ptr::null(),wide.as_mut_ptr() as H,&mut bytes,ptr::null_mut()
    );
    if ok==0 {return None;}
    let end=wide.iter().position(|x|*x==0).unwrap_or(wide.len());
    Some(String::from_utf16_lossy(&wide[..end]))
}
unsafe extern "system" fn on_receive(h:H,reserved:H)->B {
    let real = original!(RECEIVE, ReceiveFn);
    let ok = real(h,reserved);
    if ok==0 {return ok;}
    let (rule,body) = {
        let state = tables().lock().unwrap_or_else(|e|e.into_inner());
        let req = match state.requests.get(&state_key(h)) {Some(r)=>r,None=>return ok};
        if req.async_session {return ok;}
        let runtime=match RUNTIME.get(){Some(r)=>r,None=>return ok};
        let rule=match runtime.cfg.observe_rule(&req.host,&req.path,&req.method){
            Some(r)=>r,None=>return ok
        };
        if !rule.enabled {
            logger(&format!("{}: observed; overlay disabled; passthrough",rule.name));
            return ok;
        }
        (rule,req.request_body.clone())
    };
    let runtime=match RUNTIME.get(){Some(r)=>r,None=>return ok};
    let status=query_number(h,QUERY_STATUS_CODE).unwrap_or(0);
    if status != 200 && rule.mode != Mode::CatalogItem {
        logger(&format!("{}: upstream status {}; passthrough",rule.name,status));return ok;
    }
    if let Some(encoding)=query_string(h,QUERY_CONTENT_ENCODING) {
        if !encoding.is_empty() && !encoding.eq_ignore_ascii_case("identity") {
            logger(&format!("{}: encoded response ({encoding}); passthrough",rule.name));
            return ok;
        }
    }
    let bound=runtime.cfg.max_capture_bytes.clamp(1024,16*1024*1024);
    if query_number(h,QUERY_CONTENT_LENGTH).is_some_and(|n|n as usize>bound) {
        logger(&format!("{}: response exceeds capture limit; passthrough",rule.name));
        return ok;
    }
    let mut original=Vec::new();
    let mut complete=false;
    let reader=original!(READ, ReadFn);
    while original.len()<bound {
        let mut chunk=[0u8;READ_CHUNK];
        let mut n=0u32;
        let cap=chunk.len().min(bound-original.len()) as u32;
        let success=reader(h,chunk.as_mut_ptr() as H,cap,&mut n);
        if success==0 {break;}
        if n==0 {complete=true;break;}
        if n>cap {break;}
        original.extend_from_slice(&chunk[..n as usize]);
    }
    let mut replaced=false;
    let mut final_body=original;
    if complete {
        match response::overlay(&runtime.root,&rule,&body,&final_body) {
            Ok(Some(modified)) if modified.len() <= bound => {
                final_body=modified;
                replaced=true;
                logger(&format!("{}: applied local overlay ({} bytes)",rule.name,final_body.len()));
            }
            Ok(Some(_)) => logger(&format!("{}: replacement too large; original preserved",rule.name)),
            Ok(None) => logger(&format!("{}: no additions; original preserved",rule.name)),
            Err(e) => logger(&format!("{}: overlay rejected ({e}); original preserved",rule.name)),
        }
    } else {
        logger(&format!("{}: incomplete buffered response; replay and passthrough",rule.name));
    }
    let replay=Replay {bytes:final_body,position:0,complete,replaced,catalog:rule.mode==Mode::CatalogItem || rule.mode==Mode::ReplaceJson};
    if let Some(req)=tables().lock().unwrap_or_else(|e|e.into_inner())
        .requests.get_mut(&state_key(h)) {
        req.replay=Some(replay);
    }
    ok
}
unsafe extern "system" fn on_read(h:H,buffer:H,cap:u32,out:*mut u32)->B {
    if !buffer.is_null() && !out.is_null() {
        let mut state=tables().lock().unwrap_or_else(|e|e.into_inner());
        if let Some(replay)=state.requests.get_mut(&state_key(h)).and_then(|r|r.replay.as_mut()) {
            let left=replay.bytes.len()-replay.position;
            if left>0 || replay.complete {
                let n=left.min(cap as usize);
                ptr::copy_nonoverlapping(replay.bytes.as_ptr().add(replay.position),buffer as *mut u8,n);
                replay.position+=n;
                *out=n as u32;
                return 1;
            }
        }
    }
    original!(READ, ReadFn)(h,buffer,cap,out)
}
unsafe extern "system" fn on_available(h:H,out:*mut u32)->B {
    if !out.is_null() {
        let state=tables().lock().unwrap_or_else(|e|e.into_inner());
        if let Some(replay)=state.requests.get(&state_key(h)).and_then(|r|r.replay.as_ref()) {
            let remain=replay.bytes.len()-replay.position;
            if remain>0 || replay.complete {
                *out=remain.min(u32::MAX as usize) as u32;
                return 1;
            }
        }
    }
    original!(AVAILABLE, AvailableFn)(h,out)
}
unsafe fn synthetic_header(query:u32,buffer:H,length:*mut u32,text:&str)->B {
    if length.is_null(){return 0;}
    if query&QUERY_NUMBER!=0 {
        let number=match text.parse::<u32>(){Ok(v)=>v,Err(_)=>return 0};
        if buffer.is_null() || *length<4 {*length=4;SetLastError(ERROR_INSUFFICIENT_BUFFER);return 0;}
        ptr::write_unaligned(buffer as *mut u32,number);
        *length=4;return 1;
    }
    let data:Vec<u16>=text.encode_utf16().chain(Some(0)).collect();
    let bytes=(data.len()*2) as u32;
    if buffer.is_null() || *length<bytes {
        *length=bytes;SetLastError(ERROR_INSUFFICIENT_BUFFER);return 0;
    }
    ptr::copy_nonoverlapping(data.as_ptr(),buffer as *mut u16,data.len());
    *length=bytes;1
}
unsafe extern "system" fn on_headers(
    h:H,query:u32,name:*const u16,buffer:H,length:*mut u32,index:*mut u32
)->B {
    if name.is_null() {
        let state=tables().lock().unwrap_or_else(|e|e.into_inner());
        if let Some(rep)=state.requests.get(&state_key(h))
            .and_then(|r|r.replay.as_ref()).filter(|r|r.replaced) {
            let code=query&0xffff;
            if code==QUERY_CONTENT_LENGTH {
                return synthetic_header(query,buffer,length,&rep.bytes.len().to_string());
            }
            if code==QUERY_STATUS_CODE && rep.catalog {
                return synthetic_header(query,buffer,length,"200");
            }
            if code==QUERY_CONTENT_TYPE && rep.catalog {
                return synthetic_header(query,buffer,length,"application/json");
            }
        }
    }
    original!(HEADERS, HeadersFn)(h,query,name,buffer,length,index)
}
unsafe extern "system" fn on_close(h:H)->B {
    let mut state=tables().lock().unwrap_or_else(|e|e.into_inner());
    state.requests.remove(&state_key(h));
    state.connections.remove(&state_key(h));
    state.sessions.remove(&state_key(h));
    drop(state);
    original!(CLOSE, CloseFn)(h)
}
