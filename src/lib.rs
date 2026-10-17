//! Standalone native DLL: no BLoader dependency or Minecraft internal symbols.
//! The response engine can be tested on any platform; hooks are Windows-only.
pub mod response;
pub mod diagnostics;
pub mod debug;
pub mod codec;
pub mod persona;
pub mod resources;
pub mod local_capes;
pub mod persona_pack;
pub mod cape_sections;

#[cfg(windows)]
mod winhttp;

#[cfg(windows)]
#[no_mangle]
pub unsafe extern "system" fn DllMain(
    module: *mut std::ffi::c_void,
    reason: u32,
    _reserved: *mut std::ffi::c_void,
) -> i32 {
    if reason == 1 {
        // Only call lightweight kernel32 APIs while the loader lock is held.
        // All file I/O, logging, and hook installation happen in the worker.
        winhttp::spawn_worker(module);
    }
    1
}
