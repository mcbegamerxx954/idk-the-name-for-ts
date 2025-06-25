use std::{ffi::CStr, pin::Pin, sync::OnceLock};
mod aasset;
mod draco;
mod mbl;
mod plthook;
use crate::plthook::replace_plt_functions;
use bhook::hook_fn;
use core::mem::transmute;
use cxx::CxxString;
use libc::{android_set_abort_message, c_void};
use plt_rs::DynamicLibrary;
use proc_maps::MapRange;
use tinypatscan::Pattern;

// Setup for the log crate
pub fn setup_logging() {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Trace),
    );
}
#[ctor::ctor]
fn main() {
    setup_logging();
    log::info!("Starting");
    let backend = match mbl::startup() {
        Some(yay) => yay,
        None => draco::startup(),
    };
    aasset::BACKEND.set(backend).unwrap();
    // Pattern taken from materialbinloader
    hook_aaset();
}
macro_rules! cast_array {
    ($($func_name:literal -> $hook:expr),
        *,
    ) => {
        [
            $(($func_name, $hook as *const u8)),*,
        ]
    }
}
// Setup asset hooks
pub fn hook_aaset() {
    let lib_entry = find_lib("libminecraftpe").expect("Cannot find minecraftpe");
    let dyn_lib = DynamicLibrary::initialize(lib_entry).expect("Failed to find mc info");
    let asset_fn_list = cast_array! {
        "AAssetManager_open" -> aasset::open,
        "AAsset_read" -> aasset::read,
        "AAsset_close" -> aasset::close,
        "AAsset_seek" -> aasset::seek,
        "AAsset_seek64" -> aasset::seek64,
        "AAsset_getLength" -> aasset::len,
        "AAsset_getLength64" -> aasset::len64,
        "AAsset_getRemainingLength" -> aasset::rem,
        "AAsset_getRemainingLength64" -> aasset::rem64,
        "AAsset_openFileDescriptor" -> aasset::fd_dummy,
        "AAsset_openFileDescriptor64" -> aasset::fd_dummy64,
        "AAsset_getBuffer" -> aasset::get_buffer,
        "AAsset_isAllocated" -> aasset::is_alloc,
    };
    // Hook all aassetmanager functions
    replace_plt_functions(&dyn_lib, asset_fn_list);
}
// Find minecraftpe in dlpi
fn find_lib<'a>(target_name: &str) -> Option<plt_rs::LoadedLibrary<'a>> {
    let loaded_modules = plt_rs::collect_modules();
    loaded_modules
        .into_iter()
        .find(|lib| lib.name().contains(target_name))
}
