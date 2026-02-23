#[cfg(feature = "autofixing")]
use crate::autofixer::AssetManager;
#[cfg(feature = "mbl2")]
use crate::mbl::StackString;
use crate::{opt_path_join, BackendFn, LockResultExt};
use libc::{off64_t, off_t};
use ndk_sys::{AAsset, AAssetManager};
use std::{
    borrow::Cow,
    collections::HashMap,
    ffi::{CStr, OsStr},
    fs::File,
    io::{self, Cursor, Read, Seek, Write},
    os::unix::ffi::OsStrExt,
    path::Path,
    sync::{LazyLock, Mutex, OnceLock},
};

pub static BACKEND: OnceLock<BackendFn> = OnceLock::new();

// This makes me feel wrong... but all we will do is compare the pointer
// and the struct will be used in a mutex so  this is safe??
#[derive(PartialEq, Eq, Hash)]
struct AAssetPtr(*const ndk_sys::AAsset);
unsafe impl Send for AAssetPtr {}

// The assets we have registered to replace data about
static WANTED_ASSETS: LazyLock<Mutex<HashMap<AAssetPtr, CowFile>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

macro_rules! folder_list {
    ($( apk: $apk_folder:literal -> pack: $pack_folder:expr),
        *,
    ) => {
        [
            $(($apk_folder, $pack_folder)),*,
        ]
    }
}
pub unsafe fn open(
    man: *mut AAssetManager,
    fname: *const libc::c_char,
    mode: libc::c_int,
) -> *mut ndk_sys::AAsset {
    // This is where ub can happen, but we are merely a hook.
    let aasset = unsafe { ndk_sys::AAssetManager_open(man, fname, mode) };
    let c_str = unsafe { CStr::from_ptr(fname) };
    let raw_cstr = c_str.to_bytes();
    let os_str = OsStr::from_bytes(raw_cstr);
    let c_path: &Path = Path::new(os_str);
    // Extract filename
    #[cfg(feature = "autofixing")]
    let Some(os_filename) = c_path.file_name() else {
        log::warn!("Path had no filename: {c_path:?}");
        return aasset;
    };
    // This is meant to strip the new "asset" folder path so we can be compatible with other versions
    let stripped = c_path.strip_prefix("assets/").unwrap_or(c_path);
    let Some(backend) = BACKEND.get() else {
        return aasset;
    };
    #[cfg(feature = "autofixing")]
    let Some(manager_ptr) = std::ptr::NonNull::new(man) else {
        return aasset;
    };
    #[cfg(feature = "autofixing")]
    let manager = AssetManager::from_ptr(manager_ptr);
    // Folder paths to replace and with what
    let replacement_list = folder_list! {
        apk: "gui/dist/hbui/" -> pack: "hbui/",
        apk: "skin_packs/persona/" -> pack: "persona/",
        apk: "renderer/" -> pack: "renderer/",
        apk: "resource_packs/vanilla/cameras/" -> pack: "vanilla_cameras/",
    };
    if let Some((file, pack_path)) = replacement_list
        .iter()
        .find_map(|(apk, pack)| Some((stripped.strip_prefix(apk).ok()?, pack)))
    {
        let mut sus = [0; 128];
        let joined_path = opt_path_join(&mut sus, &[Path::new(pack_path), file]);
        let Some(buffer) = backend(joined_path.as_ref()) else {
            log::debug!("Cant find file {:#?}", joined_path);
            return aasset;
        };
        let buffer = match buffer {
            Ok(yay) => yay,
            Err(e) => {
                log::error!("fuck: {e}");
                return aasset;
            }
        };
        #[cfg(feature = "autofixing")]
        let buffer = if os_filename.as_encoded_bytes().ends_with(b".material.bin") {
            let buffer = buffer.into_vec().unwrap();
            let vec = crate::autofixer::process_material(manager, &buffer).unwrap_or(buffer);
            CowFile::Buffer(Cursor::new(vec))
        } else {
            buffer
        };
        let mut wanted_lock = WANTED_ASSETS.lock().ignore_poison();
        wanted_lock.insert(AAssetPtr(aasset), buffer);
        log::info!("Loaded file {:#?}", joined_path);
        // we do not clwan cxx string because cxx ceate does that for us
        return aasset;
    }
    aasset
}
/// Join paths without allocating if possible, or
/// if the joined path does not fit the buffer then just
/// allocate instead

pub unsafe fn seek64(aasset: *mut AAsset, off: off64_t, whence: libc::c_int) -> off64_t {
    let mut wanted_assets = WANTED_ASSETS.lock().ignore_poison();
    let Some(file) = wanted_assets.get_mut(&AAssetPtr(aasset)) else {
        return ndk_sys::AAsset_seek64(aasset, off, whence);
    };
    seek_facade(off, whence, file) as off64_t
}

pub unsafe fn seek(aasset: *mut AAsset, off: off_t, whence: libc::c_int) -> off_t {
    let mut wanted_assets = WANTED_ASSETS.lock().ignore_poison();
    let Some(file) = wanted_assets.get_mut(&AAssetPtr(aasset)) else {
        return ndk_sys::AAsset_seek(aasset, off, whence);
    };
    // This code can be very deadly on large files,
    // But Minecraft does not use this so we are safe 😆😆
    seek_facade(off, whence, file) as off_t
}

pub unsafe fn read(
    aasset: *mut AAsset,
    buf: *mut libc::c_void,
    count: libc::size_t,
) -> libc::c_int {
    let mut wanted_assets = WANTED_ASSETS.lock().ignore_poison();
    let Some(file) = wanted_assets.get_mut(&AAssetPtr(aasset)) else {
        return ndk_sys::AAsset_read(aasset, buf, count);
    };
    // Reuse buffer given by caller
    let rs_buffer = core::slice::from_raw_parts_mut(buf as *mut u8, count);
    match file.read(rs_buffer) {
        Ok(n) => n as libc::c_int,
        Err(e) => {
            log::warn!("failed fake aaset read: {e}");
            -1
        }
    }
}

pub unsafe fn len(aasset: *mut AAsset) -> off_t {
    let wanted_assets = WANTED_ASSETS.lock().ignore_poison();
    let Some(file) = wanted_assets.get(&AAssetPtr(aasset)) else {
        return ndk_sys::AAsset_getLength(aasset);
    };
    file.len().unwrap() as off_t
}

pub unsafe fn len64(aasset: *mut AAsset) -> off64_t {
    let wanted_assets = WANTED_ASSETS.lock().ignore_poison();
    let Some(file) = wanted_assets.get(&AAssetPtr(aasset)) else {
        return ndk_sys::AAsset_getLength64(aasset);
    };
    file.len().unwrap() as off64_t
}

pub unsafe fn rem(aasset: *mut AAsset) -> off_t {
    let mut wanted_assets = WANTED_ASSETS.lock().ignore_poison();
    let Some(file) = wanted_assets.get_mut(&AAssetPtr(aasset)) else {
        return ndk_sys::AAsset_getRemainingLength(aasset);
    };
    file.rem().unwrap() as off_t
}

pub unsafe fn rem64(aasset: *mut AAsset) -> off64_t {
    let mut wanted_assets = WANTED_ASSETS.lock().ignore_poison();
    let Some(file) = wanted_assets.get_mut(&AAssetPtr(aasset)) else {
        return ndk_sys::AAsset_getRemainingLength64(aasset);
    };
    file.rem().unwrap() as off64_t
}

pub unsafe fn close(aasset: *mut AAsset) {
    let mut wanted_assets = WANTED_ASSETS.lock().ignore_poison();
    if wanted_assets.remove(&AAssetPtr(aasset)).is_none() {
        ndk_sys::AAsset_close(aasset);
    }
}

pub unsafe fn get_buffer(aasset: *mut AAsset) -> *const libc::c_void {
    let mut wanted_assets = WANTED_ASSETS.lock().ignore_poison();
    let Some(file) = wanted_assets.get_mut(&AAssetPtr(aasset)) else {
        return ndk_sys::AAsset_getBuffer(aasset);
    };
    // Lets hope this does not go boom boom
    file.raw_buffer().unwrap().cast()
}

pub unsafe fn fd_dummy(
    aasset: *mut AAsset,
    out_start: *mut off_t,
    out_len: *mut off_t,
) -> libc::c_int {
    let wanted_assets = WANTED_ASSETS.lock().ignore_poison();
    if let None = wanted_assets.get(&AAssetPtr(aasset)) {
        ndk_sys::AAsset_openFileDescriptor(aasset, out_start, out_len)
    } else {
        log::error!("WE GOT BUSTED NOOO");
        -1
    }
}

pub unsafe fn fd_dummy64(
    aasset: *mut AAsset,
    out_start: *mut off64_t,
    out_len: *mut off64_t,
) -> libc::c_int {
    let wanted_assets = WANTED_ASSETS.lock().ignore_poison();
    if let None = wanted_assets.get(&AAssetPtr(aasset)) {
        ndk_sys::AAsset_openFileDescriptor64(aasset, out_start, out_len)
    } else {
        log::error!("WE GOT BUSTED NOOO");
        -1
    }
}

pub unsafe fn is_alloc(aasset: *mut AAsset) -> libc::c_int {
    let wanted_assets = WANTED_ASSETS.lock().ignore_poison();
    if wanted_assets.get(&AAssetPtr(aasset)).is_some() {
        false as libc::c_int
    } else {
        ndk_sys::AAsset_isAllocated(aasset)
    }
}

fn seek_facade(offset: i64, whence: libc::c_int, file: &mut CowFile) -> i64 {
    let offset = match whence {
        libc::SEEK_SET => {
            //Lets check this so we dont mess up
            let u64_off = match u64::try_from(offset) {
                Ok(uoff) => uoff,
                Err(e) => {
                    log::error!("signed ({offset}) to unsigned failed: {e}");
                    return -1;
                }
            };
            io::SeekFrom::Start(u64_off)
        }
        libc::SEEK_CUR => io::SeekFrom::Current(offset),
        libc::SEEK_END => io::SeekFrom::End(offset),
        _ => {
            log::error!("Invalid seek whence");
            return -1;
        }
    };
    match file.seek(offset) {
        Ok(new_offset) => match new_offset.try_into() {
            Ok(int) => int,
            Err(err) => {
                log::error!("u64 ({new_offset}) to i64 failed: {err}");
                -1
            }
        },
        Err(err) => {
            log::error!("aasset seek failed: {err}");
            -1
        }
    }
}

macro_rules! match_buffers {
    ($self:ident, $buf:ident,$func:expr) => {
        match $self {
            CowFile::File($buf) => $func,
            CowFile::Buffer($buf) => $func,
            #[cfg(feature = "mbl2")]
            CowFile::Cxx($buf) => $func,
        }
    };
}
// Struct that contains either a file or a buffer to read bytes from
pub enum CowFile {
    File(File),
    Buffer(Cursor<Vec<u8>>),
    #[cfg(feature = "mbl2")]
    Cxx(Cursor<StackString>),
}
impl Read for CowFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match_buffers!(self, sbuf, sbuf.read(buf))
    }
}
impl Seek for CowFile {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        match_buffers!(self, sbuf, sbuf.seek(pos))
    }
}
impl CowFile {
    fn len(&self) -> Result<u64, io::Error> {
        Ok(match self {
            Self::File(file) => file.metadata()?.len(),
            Self::Buffer(cursor) => cursor.get_ref().len() as _,
            #[cfg(feature = "mbl2")]
            Self::Cxx(cxxcursor) => cxxcursor.get_ref().as_ref().len() as _,
        })
    }
    fn rem(&mut self) -> Result<u64, io::Error> {
        Ok(self.len()? - self.stream_position()?)
    }
    fn raw_buffer(&mut self) -> Result<*mut u8, io::Error> {
        let len = self.len()? as usize;
        let mut vec = match self {
            Self::File(file) => {
                let mut vec = Vec::with_capacity(len);
                file.read_to_end(&mut vec)?;
                vec
            }
            Self::Buffer(cursor) => cursor.get_ref().clone(),
            #[cfg(feature = "mbl2")]
            Self::Cxx(cursor) => cursor.get_ref().as_ref().to_vec(),
        };
        let ptr = vec.as_mut_ptr();
        std::mem::forget(vec);
        Ok(ptr)
    }
    #[cfg(feature = "autofixing")]
    fn into_vec(self) -> io::Result<Vec<u8>> {
        match self {
            Self::File(mut f) => {
                let mut buffer = Vec::with_capacity(f.metadata()?.len() as usize);
                f.read_to_end(&mut buffer)?;
                Ok(buffer)
            }
            Self::Buffer(b) => Ok(b.into_inner()),
            #[cfg(feature = "mbl2")]
            Self::Cxx(cxx) => Ok(cxx.get_ref().as_ref().to_vec()),
        }
    }
}
