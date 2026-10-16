//! Native WinHTTP responses: bounded sync buffering, async callback capture,
//! and opt-in virtual local Persona ZIP resources. No TLS interception and no BLoader.
use crate::response::{Config, Mode};
use crate::{codec, persona};
use std::collections::HashMap;
use std::ffi::c_void;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::ptr;
use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

type H = *mut c_void;
type B = i32;
const ASYNC_FLAG: u32 = 0x1000_0000;
const QUERY_NUMBER: u32 = 0x2000_0000;
const QUERY_CONTENT_TYPE: u32 = 1;
const QUERY_CONTENT_LENGTH: u32 = 5;
const QUERY_STATUS_CODE: u32 = 19;
const QUERY_CONTENT_ENCODING: u32 = 29;
const QUERY_TRANSFER_ENCODING: u32 = 6;
const QUERY_RAW_HEADERS: u32 = 21;
const QUERY_RAW_HEADERS_CRLF: u32 = 22;
const ERROR_WINHTTP_HEADER_NOT_FOUND: u32 = 12150;
const ERROR_IO_PENDING: u32 = 997;
const STATUS_HEADERS: u32 = 0x0002_0000;
const STATUS_AVAILABLE: u32 = 0x0004_0000;
const STATUS_READ: u32 = 0x0008_0000;
const STATUS_REQUEST_ERROR: u32 = 0x0020_0000;
const STATUS_SEND_COMPLETE: u32 = 0x0040_0000;
const STATUS_HANDLE_CLOSING: u32 = 0x0000_0800;
const ADDITIONAL_CALLBACK_FLAGS: u32 = STATUS_HEADERS | STATUS_READ | STATUS_REQUEST_ERROR | STATUS_HANDLE_CLOSING;
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
    fn GetLastError() -> u32;
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
type StatusCallback = unsafe extern "system" fn(H,usize,u32,H,u32);
type SetCallbackFn = unsafe extern "system" fn(H,H,u32,usize)->H;

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
static SET_CALLBACK: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());

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
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);
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
    callbacks: HashMap<usize, CallbackRegistration>,
}
#[derive(Clone, Copy)]
struct CallbackRegistration { address: usize, flags: u32 }
struct Request {
    id: u64,
    diagnostic_route: Option<String>,
    catalog_checked: bool,
    body_overflow: bool,
    host: String,
    path: String,
    method: String,
    async_session: bool,
    request_body: Vec<u8>,
    replay: Option<Replay>,
    capture: Option<AsyncCapture>,
    synthetic: bool,
    status: u32,
    callback_context: usize,
}
struct AsyncCapture {
    bytes: Vec<u8>,
    buffer: Box<[u8; READ_CHUNK]>,
    limit: usize,
    encoding: codec::Encoding,
    pumping: bool,
    needs_read: bool,
}
struct Replay {
    bytes: Vec<u8>,
    position: usize,
    complete: bool,
    replaced: bool,
    catalog: bool,
    zip: bool,
    strip_encoding: bool,
    status: u32,
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
    let _ = RUNTIME.set(Runtime { cfg, root, log: Mutex::new(log) });
    logger("Cosmetics v0.1.1-diagnostic; independent WinHTTP module; fail-open enabled");
    logger(&format!("loaded rules={}; enabled={}; diagnostics=redacted",RUNTIME.get().unwrap().cfg.rules.len(),
        RUNTIME.get().unwrap().cfg.rules.iter().filter(|r|r.enabled).count()));
    if let Err(e) = install() {
        logger(&format!("hook install failed; game remains unmodified: {e}"));
        return 6;
    }
    logger("WinHTTP hooks installed; open the Dressing Room capes page to see a route match");
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
        (b"WinHttpSetStatusCallback\0", on_set_callback as H, &SET_CALLBACK),
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
        if let Some(cb)=state.callbacks.get(&state_key(session)).copied() {
            state.callbacks.insert(state_key(h),cb);
        }
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
        let method=if verb.is_null() { "GET".into() } else { read_wide(verb) };
        let path=read_wide(path);
        let id=NEXT_REQUEST_ID.fetch_add(1,Ordering::Relaxed);
        let diagnostic_route=RUNTIME.get().and_then(|rt|
            rt.cfg.observe_rule(&host,&path,&method).map(|rule| {
                format!("{} ({})",rule.name,if rule.enabled {"enabled"} else {"disabled"})
            })
        );
        let is_commerce_host=RUNTIME.get().is_some_and(|rt|
            rt.cfg.rules.iter().any(|rule|rule.host.eq_ignore_ascii_case(&host)));
        let req = Request {
            id, diagnostic_route: diagnostic_route.clone(), catalog_checked:false,
            body_overflow:false, host, path, method,
            async_session, request_body:Vec::new(), replay:None,
            capture:None, synthetic:false, status:200, callback_context:0,
        };
        if let Some(cb)=state.callbacks.get(&state_key(connection)).copied() {
            state.callbacks.insert(state_key(h),cb);
        }
        state.requests.insert(state_key(h),req);
        drop(state);
        if let Some(name)=diagnostic_route {
            logger(&format!("request #{id}: route matched {name}; async={async_session}"));
        } else if RUNTIME.get().is_some() {
            // Never write raw paths, query values, tokens, accounts or URLs.
            // This safely distinguishes an endpoint mismatch from no WinHTTP traffic.
            if is_commerce_host {
                logger(&format!("request #{id}: commerce host observed but path/verb not matched"));
            }
        }
    }
    h
}

// The caller's callback is stored separately; our callback is only a bridge.
// No application callback is invoked while the state mutex is held.
unsafe fn notify_game(h:H,context:usize,status:u32,info:H,len:u32) {
    let reg = {
        let state=tables().lock().unwrap_or_else(|e| e.into_inner());
        state.callbacks.get(&state_key(h)).copied()
    };
    if let Some(reg)=reg {
        if reg.address!=0 && reg.flags&status!=0 {
            let f:StatusCallback=std::mem::transmute(reg.address);
            f(h,context,status,info,len);
        }
    }
}
unsafe extern "system" fn on_set_callback(h:H,callback:H,flags:u32,reserved:usize)->H {
    let real=original!(SET_CALLBACK,SetCallbackFn);
    let old={
        tables().lock().unwrap_or_else(|e|e.into_inner()).callbacks.get(&state_key(h)).copied()
    };
    let is_normal=!callback.is_null() && callback as usize!=usize::MAX;
    let replacement=if is_normal { status_bridge as H } else { callback };
    let effective_flags=if is_normal {flags|ADDITIONAL_CALLBACK_FLAGS} else {flags};
    let previous=real(h,replacement,effective_flags,reserved);
    if previous as usize==usize::MAX {return previous;}
    let mut state=tables().lock().unwrap_or_else(|e|e.into_inner());
    if is_normal {
        state.callbacks.insert(state_key(h),CallbackRegistration{address:callback as usize,flags});
    } else {
        state.callbacks.remove(&state_key(h));
    }
    old.map(|r|r.address as H).unwrap_or(previous)
}
unsafe fn invoke_headers(h:H,context:usize) {
    notify_game(h,context,STATUS_HEADERS,ptr::null_mut(),0);
}
unsafe fn catalog_fast_response(h:H)->bool {
    let Some(runtime)=RUNTIME.get() else{return false};
    // Never diagnose the same item twice (ReceiveResponse and HEADERS callback).
    let found={
        let mut state=tables().lock().unwrap_or_else(|e|e.into_inner());
        let Some(r)=state.requests.get_mut(&state_key(h)) else{return false};
        if r.catalog_checked{return r.replay.as_ref().is_some_and(|rep|rep.replaced && rep.catalog);}
        let Some(rule)=runtime.cfg.match_rule(&r.host,&r.path,&r.method) else{return false};
        if !matches!(rule.mode,Mode::CatalogItem|Mode::ReplaceJson){return false;}
        r.catalog_checked=true;
        (r.id,rule,r.request_body.clone(),r.body_overflow)
    };
    let (id,rule,body,overflow)=found;
    let structure=crate::response::catalog_request_state(&body);
    logger(&format!("request #{id}: catalog lookup {} body_bytes={} body_state={structure} overflow={overflow}",rule.name,body.len()));
    if overflow {return false;}
    // CatalogItem should only replace a registered local ID. Unknown official
    // item requests stay on the official PlayFab response path.
    if rule.mode==Mode::CatalogItem && structure!="valid_itemid" {
        logger(&format!("request #{id}: catalog not modified; check request body capture"));
        return false;
    }
    match crate::response::overlay(&runtime.root,&rule,&body,b"") {
        Ok(Some(bytes)) if bytes.len()<=runtime.cfg.max_capture_bytes => {
            let mut state=tables().lock().unwrap_or_else(|e|e.into_inner());
            if let Some(r)=state.requests.get_mut(&state_key(h)){
                r.replay=Some(Replay{bytes,position:0,complete:true,replaced:true,
                    catalog:true,zip:false,strip_encoding:false,status:200});
            }
            logger(&format!("request #{id}: catalog local response applied ({})",rule.name));
            true
        }
        Ok(None)=>{
            logger(&format!("request #{id}: catalog item not registered locally; passthrough"));
            false
        }
        Ok(Some(_))=>{
            logger(&format!("request #{id}: catalog local response exceeds bound; passthrough"));
            false
        }
        Err(e)=>{
            logger(&format!("request #{id}: catalog response rejected ({e}); passthrough"));
            false
        }
    }
}
unsafe extern "system" fn on_send(
    h:H,headers:*const u16,header_len:u32,optional:H,optional_len:u32,total_len:u32,ctx:usize
)->B {
    let mut captured:Option<(u64,String,usize,usize,bool)>=None;
    {
        let mut state=tables().lock().unwrap_or_else(|e|e.into_inner());
        if let Some(req)=state.requests.get_mut(&state_key(h)) {
            if optional_len>0 && !optional.is_null() {
                let count=optional_len as usize;
                if count <= REQUEST_BODY_MAX.saturating_sub(req.request_body.len()) {
                    let bytes=std::slice::from_raw_parts(optional as *const u8,count);
                    req.request_body.extend_from_slice(bytes);
                } else {
                    req.body_overflow=true;
                }
            }
            if let Some(route)=req.diagnostic_route.as_ref() {
                captured=Some((req.id,route.clone(),req.request_body.len(),total_len as usize,req.body_overflow));
            }
        }
    }
    if let Some((id,route,bytes,expected,overflow))=captured {
        logger(&format!("request #{id}: send {route}; captured_body_bytes={bytes}; declared_total_bytes={expected}; overflow={overflow}"));
    }
    let virtual_resource={
        let state=tables().lock().unwrap_or_else(|e|e.into_inner());
        state.requests.get(&state_key(h)).and_then(|r| {
            if r.host.eq_ignore_ascii_case(persona::LOCAL_HOST) {
                Some((r.path.clone(),r.method.clone(),r.async_session))
            }else{None}
        })
    };
    if let Some((path,method,async_request))=virtual_resource {
        let loaded=if method.eq_ignore_ascii_case("GET"){
            RUNTIME.get().map(|runtime|persona::load(&runtime.root,&path))
        }else{None};
        let (data,status)=match loaded {
            Some(Ok(Some(resource)))=>{
                logger(&format!("Persona ZIP found: {} ({} bytes)",resource.resource_uuid,resource.bytes.len()));
                (resource.bytes,200)
            }
            Some(Err(e))=>{logger(&format!("Persona ZIP rejected: {e}"));(Vec::new(),404)}
            _=>{logger("local Persona ZIP not found or invalid request");(Vec::new(),404)}
        };
        {
            let mut state=tables().lock().unwrap_or_else(|e|e.into_inner());
            if let Some(r)=state.requests.get_mut(&state_key(h)) {
                r.synthetic=true;r.status=status;r.callback_context=ctx;
                r.replay=Some(Replay{bytes:data,position:0,complete:true,replaced:true,
                    catalog:false,zip:true,strip_encoding:false,status});
            }
        }
        if async_request {notify_game(h,ctx,STATUS_SEND_COMPLETE,ptr::null_mut(),0);}
        return 1;
    }
    if let Some(r)=tables().lock().unwrap_or_else(|e|e.into_inner())
        .requests.get_mut(&state_key(h)){r.callback_context=ctx;}
    original!(SEND, SendFn)(h,headers,header_len,optional,optional_len,total_len,ctx)
}
unsafe extern "system" fn on_write(h:H,data:*const c_void,len:u32,written:*mut u32)->B {
    // Synthetic Persona downloads are GET-only. No write is ever sent to DNS.
    if tables().lock().unwrap_or_else(|e|e.into_inner())
        .requests.get(&state_key(h)).is_some_and(|r|r.synthetic) {
        return 0;
    }
    let result=original!(WRITE,WriteFn)(h,data,len,written);
    let error=if result==0 {GetLastError()}else{0};
    if !data.is_null() && len>0 {
        let async_request=tables().lock().unwrap_or_else(|e|e.into_inner())
            .requests.get(&state_key(h)).is_some_and(|r|r.async_session);
        // In an async WinHTTP session, ERROR_IO_PENDING indicates the queued
        // write uses the caller's live buffer. Copy it now, before it is freed.
        // The diagnostic describes it as queued, not as a confirmed delivery.
        let accepted=result!=0 || (async_request && error==ERROR_IO_PENDING);
        if accepted {
            let count=if async_request {len as usize}
                else if written.is_null(){0}else{(*written as usize).min(len as usize)};
            let mut message=None;
            {
                let mut state=tables().lock().unwrap_or_else(|e|e.into_inner());
                if let Some(req)=state.requests.get_mut(&state_key(h)) {
                    if count<=REQUEST_BODY_MAX.saturating_sub(req.request_body.len()) {
                        req.request_body.extend_from_slice(std::slice::from_raw_parts(data as *const u8,count));
                    } else {req.body_overflow=true;}
                    if let Some(route)=req.diagnostic_route.as_ref() {
                        message=Some(format!("request #{}: write {route}; submitted_bytes={count}; captured_body_bytes={}; queued={}; overflow={}",
                            req.id,req.request_body.len(),result==0,req.body_overflow));
                    }
                }
            }
            if let Some(message)=message {logger(&message);}
        }
    }
    if result==0 {SetLastError(error);}
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

unsafe fn begin_async_capture(h:H,context:usize)->bool {
    let Some(runtime)=RUNTIME.get() else{return false};
    let rule={
        let state=tables().lock().unwrap_or_else(|e|e.into_inner());
        let Some(req)=state.requests.get(&state_key(h)) else{return false};
        if !req.async_session{return false;}
        let Some(rule)=runtime.cfg.match_rule(&req.host,&req.path,&req.method) else{return false};
        if rule.mode!=Mode::AppendArray {return false;}
        rule
    };
    let status=query_number(h,QUERY_STATUS_CODE);
    if status!=Some(200) {
        logger(&format!("{}: async headers status={status:?}; passthrough",rule.name));
        return false;
    }
    let encoding=match codec::Encoding::parse(&query_string(h,QUERY_CONTENT_ENCODING).unwrap_or_default()) {
        Ok(c)=>c,Err(e)=>{logger(&format!("{}: {e}; passthrough",rule.name));return false}
    };
    let max=runtime.cfg.max_capture_bytes.clamp(1024,16*1024*1024);
    if query_number(h,QUERY_CONTENT_LENGTH).is_some_and(|n|n as usize>max) {
        logger(&format!("{}: response larger than capture limit",rule.name));
        return false;
    }
    {
        let mut state=tables().lock().unwrap_or_else(|e|e.into_inner());
        let Some(req)=state.requests.get_mut(&state_key(h)) else{return false};
        if req.capture.is_some()||req.replay.is_some(){return false;}
        req.callback_context=context;
        req.capture=Some(AsyncCapture {
            bytes:Vec::new(),buffer:Box::new([0;READ_CHUNK]),
            limit:max,encoding,pumping:false,needs_read:true
        });
    }
    logger(&format!("{}: async prefetch started",rule.name));
    pump_async(h);
    true
}
unsafe fn pump_async(h:H) {
    {
        let mut state=tables().lock().unwrap_or_else(|e|e.into_inner());
        let Some(cap)=state.requests.get_mut(&state_key(h)).and_then(|r|r.capture.as_mut()) else{return};
        if cap.pumping{return;}
        cap.pumping=true;
    }
    loop {
        let buf={
            let mut state=tables().lock().unwrap_or_else(|e|e.into_inner());
            let Some(cap)=state.requests.get_mut(&state_key(h)).and_then(|r|r.capture.as_mut()) else{return};
            if !cap.needs_read {cap.pumping=false;return;}
            cap.needs_read=false;
            cap.buffer.as_mut_ptr()
        };
        // The stable Box owns the buffer until HANDLE_CLOSING. This call may
        // complete inline (reentrant callback) or later on a WinHTTP thread.
        let success=original!(READ,ReadFn)(h,buf as H,READ_CHUNK as u32,ptr::null_mut());
        if success==0 {
            let error=GetLastError();
            if error!=ERROR_IO_PENDING {
                logger("asynchronous WinHttpReadData failed; replay buffered original bytes");
                finish_async(h,false);
                return;
            }
        }
        let continue_now={
            let mut state=tables().lock().unwrap_or_else(|e|e.into_inner());
            let Some(cap)=state.requests.get_mut(&state_key(h)).and_then(|r|r.capture.as_mut()) else{return};
            if cap.needs_read {true}else{cap.pumping=false;false}
        };
        if !continue_now{return;}
    }
}
unsafe fn finish_async(h:H,completed:bool) {
    let Some(runtime)=RUNTIME.get() else{return};
    let taken={
        let mut state=tables().lock().unwrap_or_else(|e|e.into_inner());
        let Some(req)=state.requests.get_mut(&state_key(h)) else{return};
        let Some(cap)=req.capture.take() else{return};
        // Keep the stable storage alive until WinHTTP has reported completion.
        // There must not be an outstanding read when this path is reached.
        (req.host.clone(),req.path.clone(),req.method.clone(),req.request_body.clone(),
         req.callback_context,cap.bytes,cap.encoding)
    };
    let (host,path,method,body,context,original,encoding)=taken;
    let rule=runtime.cfg.match_rule(&host,&path,&method);
    let mut replay=Replay{
        bytes:original,position:0,complete:completed,replaced:false,catalog:false,zip:false,
        strip_encoding:false,status:200
    };
    if completed {
        if let Some(rule)=rule {
            match codec::rewrite(&runtime.root,&rule,&body,&replay.bytes,encoding,
                runtime.cfg.max_capture_bytes.clamp(1024,16*1024*1024)) {
                Ok(Some(change)) => {
                    replay.bytes=change.body;
                    replay.replaced=true;
                    replay.strip_encoding=change.strip_encoding;
                    logger(&format!("{}: async response overlay applied",rule.name));
                }
                Ok(None)=>logger(&format!("{}: async response unchanged",rule.name)),
                Err(e)=>logger(&format!("{}: async overlay failed; original kept ({e})",rule.name)),
            }
        }
    }
    {
        let mut state=tables().lock().unwrap_or_else(|e|e.into_inner());
        if let Some(req)=state.requests.get_mut(&state_key(h)){req.replay=Some(replay);}
    }
    invoke_headers(h,context);
}
unsafe extern "system" fn status_bridge(h:H,context:usize,status:u32,info:H,len:u32) {
    if status==STATUS_HEADERS {
        let note={
            let state=tables().lock().unwrap_or_else(|e|e.into_inner());
            state.requests.get(&state_key(h)).and_then(|r|
                r.diagnostic_route.as_ref().map(|route|(r.id,route.clone(),r.async_session)))
        };
        if let Some((id,route,asynchronous))=note {
            logger(&format!("request #{id}: response headers callback {route}; async={asynchronous}"));
        }
        if catalog_fast_response(h) {
            invoke_headers(h,context);
            return;
        }
        if begin_async_capture(h,context) {return;}
    } else if status==STATUS_READ {
        let action={
            let mut state=tables().lock().unwrap_or_else(|e|e.into_inner());
            if let Some(cap)=state.requests.get_mut(&state_key(h)).and_then(|r|r.capture.as_mut()) {
                if info==cap.buffer.as_mut_ptr() as H {
                    if len==0 {Some(0)}
                    else if len as usize>READ_CHUNK || cap.bytes.len().saturating_add(len as usize)>cap.limit {
                        Some(2)
                    } else {
                        cap.bytes.extend_from_slice(std::slice::from_raw_parts(info as *const u8,len as usize));
                        cap.needs_read=true;
                        Some(1)
                    }
                }else{None}
            }else{None}
        };
        match action {
            Some(0)=>{finish_async(h,true);return;}
            Some(1)=>{pump_async(h);return;}
            Some(2)=>{logger("async response exceeded size limit");finish_async(h,false);return;}
            _=>{}
        }
    } else if status==STATUS_REQUEST_ERROR {
        let active={
            tables().lock().unwrap_or_else(|e|e.into_inner()).requests.get(&state_key(h))
                .is_some_and(|r|r.capture.is_some())
        };
        if active {finish_async(h,false);return;}
    }
    notify_game(h,context,status,info,len);
    if status==STATUS_HANDLE_CLOSING {
        let mut state=tables().lock().unwrap_or_else(|e|e.into_inner());
        state.requests.remove(&state_key(h));
        state.connections.remove(&state_key(h));
        state.sessions.remove(&state_key(h));
        state.callbacks.remove(&state_key(h));
    }
}

unsafe extern "system" fn on_receive(h:H,reserved:H)->B {
    let synthetic={
        let state=tables().lock().unwrap_or_else(|e|e.into_inner());
        state.requests.get(&state_key(h)).filter(|r|r.synthetic)
            .map(|r|(r.async_session,r.callback_context))
    };
    if let Some((async_req,context))=synthetic {
        if async_req{invoke_headers(h,context);}
        return 1;
    }
    let ok=original!(RECEIVE,ReceiveFn)(h,reserved);
    if ok==0{return ok;}
    let note={
        let state=tables().lock().unwrap_or_else(|e|e.into_inner());
        state.requests.get(&state_key(h)).and_then(|r|
            r.diagnostic_route.as_ref().map(|route|(r.id,route.clone(),r.async_session)))
    };
    if let Some((id,route,asynchronous))=note {
        logger(&format!("request #{id}: WinHttpReceiveResponse ok; route={route}; async={asynchronous}"));
    }
    if catalog_fast_response(h) {return ok;}
    let (rule,body)={
        let state=tables().lock().unwrap_or_else(|e|e.into_inner());
        let Some(req)=state.requests.get(&state_key(h)) else{return ok};
        if req.async_session{return ok;}
        let Some(rt)=RUNTIME.get() else{return ok};
        let Some(rule)=rt.cfg.observe_rule(&req.host,&req.path,&req.method) else{return ok};
        if !rule.enabled {
            logger(&format!("{}: response observed; overlay disabled",rule.name));
            return ok;
        }
        (rule,req.request_body.clone())
    };
    let Some(rt)=RUNTIME.get() else{return ok};
    if query_number(h,QUERY_STATUS_CODE)!=Some(200) {
        logger(&format!("{}: unexpected upstream status; passthrough",rule.name));return ok;
    }
    let encoding=match codec::Encoding::parse(&query_string(h,QUERY_CONTENT_ENCODING).unwrap_or_default()) {
        Ok(v)=>v,Err(e)=>{logger(&format!("{}: {e}",rule.name));return ok}
    };
    let limit=rt.cfg.max_capture_bytes.clamp(1024,16*1024*1024);
    if query_number(h,QUERY_CONTENT_LENGTH).is_some_and(|n|n as usize>limit) {
        logger(&format!("{}: content length exceeds limit",rule.name));return ok;
    }
    let mut raw=Vec::new();
    let mut completed=false;
    loop {
        let remaining=(limit+1).saturating_sub(raw.len());
        if remaining==0{break;}
        let mut temp=[0u8;READ_CHUNK];
        let capacity=temp.len().min(remaining) as u32;
        let mut bytes=0u32;
        let success=original!(READ,ReadFn)(h,temp.as_mut_ptr() as H,capacity,&mut bytes);
        if success==0 || bytes>capacity {break;}
        if bytes==0 {completed=true;break;}
        raw.extend_from_slice(&temp[..bytes as usize]);
    }
    let mut replay=Replay {bytes:raw,position:0,complete:completed,replaced:false,
        catalog:false,zip:false,strip_encoding:false,status:200};
    if completed && replay.bytes.len()<=limit {
        match codec::rewrite(&rt.root,&rule,&body,&replay.bytes,encoding,limit) {
            Ok(Some(change)) => {
                replay.bytes=change.body;
                replay.replaced=true;replay.strip_encoding=change.strip_encoding;
                logger(&format!("{}: synchronous JSON overlay applied",rule.name));
            }
            Ok(None)=>logger(&format!("{}: no local additions",rule.name)),
            Err(e)=>logger(&format!("{}: overlay failed, replay original ({e})",rule.name)),
        }
    } else {
        logger(&format!("{}: response not completely buffered; replay original data",rule.name));
    }
    if let Some(req)=tables().lock().unwrap_or_else(|e|e.into_inner())
        .requests.get_mut(&state_key(h)) {req.replay=Some(replay);}
    ok
}
unsafe extern "system" fn on_read(h:H,buffer:H,cap:u32,out:*mut u32)->B {
    if buffer.is_null() && cap>0 {return original!(READ,ReadFn)(h,buffer,cap,out);}
    let response={
        let mut state=tables().lock().unwrap_or_else(|e|e.into_inner());
        state.requests.get_mut(&state_key(h)).and_then(|req|{
            let async_req=req.async_session;
            let ctx=req.callback_context;
            req.replay.as_mut().and_then(|rep|{
                let remain=rep.bytes.len().saturating_sub(rep.position);
                if remain==0 && !rep.complete{return None;}
                let length=remain.min(cap as usize);
                if length>0 && !buffer.is_null() {
                    ptr::copy_nonoverlapping(rep.bytes.as_ptr().add(rep.position),buffer as *mut u8,length);
                }
                rep.position+=length;
                Some((length as u32,async_req,ctx))
            })
        })
    };
    if let Some((count,async_req,ctx))=response {
        if !out.is_null() {*out=count;}
        if async_req {notify_game(h,ctx,STATUS_READ,buffer,count);}
        return 1;
    }
    original!(READ,ReadFn)(h,buffer,cap,out)
}
unsafe extern "system" fn on_available(h:H,out:*mut u32)->B {
    let replay={
        let state=tables().lock().unwrap_or_else(|e|e.into_inner());
        state.requests.get(&state_key(h)).and_then(|r|{
            r.replay.as_ref().and_then(|rep|{
                let remain=rep.bytes.len().saturating_sub(rep.position);
                if remain==0 && !rep.complete {return None;}
                Some((remain.min(u32::MAX as usize) as u32,r.async_session,r.callback_context))
            })
        })
    };
    if let Some((count,async_req,ctx))=replay{
        if !out.is_null(){*out=count;}
        if async_req {
            let mut local=count;
            notify_game(h,ctx,STATUS_AVAILABLE,&mut local as *mut u32 as H,4);
        }
        return 1;
    }
    original!(AVAILABLE,AvailableFn)(h,out)
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
            let size=rep.bytes.len().to_string();
            let content_type=if rep.zip {"application/zip"} else {"application/json"};
            if code==QUERY_CONTENT_LENGTH {
                return synthetic_header(query,buffer,length,&size);
            }
            if code==QUERY_STATUS_CODE {
                return synthetic_header(query,buffer,length,&rep.status.to_string());
            }
            if code==QUERY_CONTENT_TYPE {
                return synthetic_header(query,buffer,length,content_type);
            }
            if rep.zip || rep.strip_encoding {
                if matches!(code, QUERY_CONTENT_ENCODING|QUERY_TRANSFER_ENCODING) {
                    SetLastError(ERROR_WINHTTP_HEADER_NOT_FOUND);
                    return 0;
                }
            }
            if code==QUERY_RAW_HEADERS_CRLF && rep.zip {
                let raw=format!("HTTP/1.1 {} OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n",
                    rep.status,content_type,size);
                return synthetic_header(query,buffer,length,&raw);
            }
            if code==QUERY_RAW_HEADERS && rep.zip {
                // This is a MULTI_SZ response.
                let raw=format!("HTTP/1.1 {} OK\0Content-Type: {}\0Content-Length: {}\0\0",
                    rep.status,content_type,size);
                return synthetic_header(query,buffer,length,&raw);
            }
        }
    }
    original!(HEADERS,HeadersFn)(h,query,name,buffer,length,index)
}
unsafe extern "system" fn on_close(h:H)->B {
    let keep_capture={
        let state=tables().lock().unwrap_or_else(|e|e.into_inner());
        state.requests.get(&state_key(h)).is_some_and(|r|r.capture.is_some())
            || state.callbacks.contains_key(&state_key(h))
    };
    if !keep_capture {
        let mut state=tables().lock().unwrap_or_else(|e|e.into_inner());
        state.requests.remove(&state_key(h));
        state.connections.remove(&state_key(h));
        state.sessions.remove(&state_key(h));
        state.callbacks.remove(&state_key(h));
    }
    original!(CLOSE,CloseFn)(h)
}
