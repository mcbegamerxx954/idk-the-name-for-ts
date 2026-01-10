#[cfg(feature = "jni")]
mod jniopts;
use std::{path::Path, sync::LockResult};
mod aasset;
#[cfg(feature = "autofixing")]
mod autofixer;
#[cfg(feature = "draco")]
mod draco;
#[cfg(feature = "mbl2")]
mod mbl;
mod plthook;
use crate::{aasset::CowFile, plthook::replace_plt_functions};
use plt_rs::DynamicLibrary;
pub type BackendFn = fn(name: &Path) -> Option<std::io::Result<CowFile>>;
#[cfg(feature = "logging")]
// Setup for the log crate
pub fn setup_logging() {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Trace),
    );
}
#[cfg(all(not(feature = "mbl2"), not(feature = "draco")))]
compile_error!("Comeon, enable either mbl2 or draco feature, or else this projevt is useless");
#[cfg(not(any(target_os = "linux", target_os = "android")))]
compile_error!("This project does nto support the target system.");

#[ctor::ctor]
fn safe_setup() {
    std::panic::set_hook(Box::new(move |panic_info| {
        log::error!("Thread crashed: {}", panic_info);
    }));
    let start = std::panic::catch_unwind(|| {
        main();
    });
    if let Err(e) = start {
        if let Ok(err) = e.downcast::<String>() {
            log::error!("Thread crash, error: {err}");
        }
    }
}

fn main() {
    #[cfg(feature = "logging")]
    setup_logging();
    log::info!("Starting");
    #[cfg(all(feature = "mbl2", feature = "draco"))]
    let backend = match mbl::startup() {
        Some(yay) => yay,
        None => {
            log::warn!("Mbl2 start failed, using draco");
            draco::startup()
        }
    };
    #[cfg(all(feature = "mbl2", not(feature = "draco")))]
    let backend = mbl::startup().unwrap();
    #[cfg(all(feature = "draco", not(feature = "mbl2")))]
    let backend = draco::startup();
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
pub trait LockResultExt {
    type Guard;
    fn ignore_poison(self) -> Self::Guard;
}

impl<Guard> LockResultExt for LockResult<Guard> {
    type Guard = Guard;
    /// You might ask: what tf is this for, simple, poisoning is useless 99% of the time
    fn ignore_poison(self) -> Guard {
        self.unwrap_or_else(|e| e.into_inner())
    }
}
fn find_lib<'a>(target_name: &str) -> Option<plt_rs::LoadedLibrary<'a>> {
    let loaded_modules = plt_rs::collect_modules();
    loaded_modules
        .into_iter()
        .find(|lib| lib.name().contains(target_name))
}
