pub mod common;
pub mod errors;
pub mod storage;
pub mod utils;
use crate::aasset::CowFile;
use crate::draco::utils::ResourcePath;
use crate::{BackendFn, LockResultExt};

use self::storage::{parse_storage_location, StorageLocation};
use bhook::hook_fn;
use jni::errors::Result as JniResult;
use jni::objects::JObject;
use jni::JNIEnv;
use libloading::{Library, Symbol};
use std::borrow::Cow;
use std::collections::HashSet;
use std::error::Error;
use std::fs::File;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;
use std::{fs, io};
#[derive(Debug)]
struct JniPaths {
    internal_path: String,
    external_path: String,
}
type IsEduFn = unsafe extern "C" fn(JNIEnv, JObject);
static JNI_PATHS: OnceLock<JniPaths> = OnceLock::new();

hook_fn! {
    fn is_edu_hook(env: jni::JNIEnv, thiz: jni::objects::JObject) ->() = {
        let mut env = env;
        if let Err(err) =  super::is_edu_hk(&mut env, &thiz) {
            log::error!("Getting jni paths failed, draco will not work: {err}");
        }
        call_original(env, thiz);
        self_disable();
    }
}
pub fn is_edu_hk(env: &mut JNIEnv, thiz: &JObject) -> Result<(), Box<dyn Error>> {
    let external_path = get_jni_path(env, &thiz, "getExternalStoragePath")?;
    let internal_path = get_jni_path(env, &thiz, "getInternalStoragePath")?;
    let paths = JniPaths {
        internal_path,
        external_path,
    };
    JNI_PATHS
        .set(paths)
        .map_err(|_| "Jni paths was already set!")?;
    Ok(())
}
fn get_jni_path(
    env: &mut jni::JNIEnv,
    instance: &jni::objects::JObject,
    fn_name: &str,
) -> JniResult<String> {
    let jstring = env
        .call_method(instance, fn_name, "()Ljava/lang/String;", &[])?
        .l()?;
    let path_str = env.get_string(jstring.as_ref().into())?;
    Ok(path_str.to_str().unwrap().to_owned())
}
pub fn get_storage_location(options_path: &Path) -> Option<StorageLocation> {
    let int = parse_storage_location(options_path)
        .inspect_err(|e| log::error!("Cant parse storage: {e}"))
        .ok()?;
    StorageLocation::from_i8(int)
}

// Get the full path for a storage location
pub fn get_storage_path(location: StorageLocation) -> std::path::PathBuf {
    let paths: &JniPaths;
    loop {
        if let Some(jni_paths) = JNI_PATHS.get() {
            paths = jni_paths;
            break;
        } else {
            log::warn!("we going slwepy time");
            std::thread::sleep(Duration::from_millis(500));
        }
    }
    let result = match location {
        StorageLocation::Internal => paths.internal_path.to_owned(),
        StorageLocation::External => paths.external_path.to_owned(),
    };
    log::info!("Jni path for {location:#?}: {}", &result);
    result.into()
}

// Get app directory for the current platform
pub fn get_path() -> std::path::PathBuf {
    get_storage_path(StorageLocation::Internal)
}

// unsafe fn open_hook(filename: *const u8, mode: *const u8) -> *mut libc::FILE {
//     let cfilename = CStr::from_ptr(filename);
//     let Osstr = OsStr::from_bytes(&cfilename.to_bytes());
//     let path = Path::new(Osstr);
//     if path
//         .file_name()
//         .is_some_and(|osstr| osstr.as_encoded_bytes().ends_with(b"options.txt"))
//     {
//         log::info!("mc opened options.txt at {:?}", path);
//     }
//     libc::fopen(filename, mode)
// }

unsafe fn special_hook(libname: &str) {
    const IS_EDU: &[u8] = b"Java_com_mojang_minecraftpe_MainActivity_isEduMode\0";
    let lib = Library::new(libname).unwrap();
    let sym: Symbol<IsEduFn> = lib.get(IS_EDU).unwrap();
    let addr = *sym;
    is_edu_hook::hook_address(addr as *mut u8);
}

static SHADER_PATHS: LazyLock<Mutex<HashSet<ResourcePath<'static>>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

pub fn startup() -> BackendFn {
    log::info!("Starting up!");
    log::info!("Finished hooking..");
    unsafe {
        special_hook("libminecraftpe.so");
    }
    std::thread::spawn(|| {
        let mut path = self::get_path();
        path.extend(["games", "com.mojang", "minecraftpe"]);
        log::info!("non verified path: {:#?}", &path);
        if !path.exists() {
            if let Err(e) = fs::create_dir_all(&path) {
                log::error!("Fatal: path to minecraftpe cant be created: {e}");
                log::error!("Quitting..");
                return;
            }
        }
        log::debug!("path is: {:#?}", &path);
        // we do it here so mcbe stays sleep while we work
        common::setup_json_watcher(path);
    });
    draco_callback
}
fn draco_callback(path: &Path) -> Option<io::Result<CowFile>> {
    let sus = SHADER_PATHS.lock().ignore_poison();
    let aah = ResourcePath::new_nameless(Cow::Borrowed(path));

    let filename = sus.get(&aah)?;
    let file = match File::open(filename.path()) {
        Ok(yay) => yay,
        Err(e) => return Some(Err(e)),
    };
    Some(Ok(CowFile::File(file)))
}
