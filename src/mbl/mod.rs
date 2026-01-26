mod cxx_utils;
pub use cxx_utils::StackString;
mod loader;
use crate::mbl::cxx_utils::ResourceLocation;
use crate::{aasset::CowFile, mbl::loader::ResourcePackManager};
use crate::{BackendFn, LockResultExt};
use atoi::FromRadix16;
use bhook::hook_fn;
use bstr::ByteSlice;
use core::slice;
use std::io::Cursor;
use std::{fs, sync::Mutex};
use tinypatscan::Pattern;

#[cfg(target_arch = "aarch64")]
const RPMC_PATTERNS: [Pattern; 3] = [
    //1.21.120.4
    Pattern::from_str("FF ?? 02 D1 FD 7B ?? A9 ?? ?? ?? ?? FA 67 ?? A9 F8 5F ?? A9 F6 57 ?? A9 F4 4F ?? A9 FD ?? 01 91 ?? D0 3B D5 ?? 03 03 2A ?? 03 02 AA ?? 17 40 F9 F3 03 00 AA A8 83 1F F8"),
    // V1.21.60.21
    Pattern::from_str("FF 83 02 D1 FD 7B 06 A9 FD 83 01 91 F8 5F 07 A9 F6 57 08 A9 F4 4F 09 A9 58 D0 3B D5 F6 03 03 2A 08 17 40 F9 F5 03 02 AA F3 03 00 AA A8 83 1F F8 28 10 40 F9 28 01 00 B4"),
    // V1.19.50-1.21.50
    Pattern::from_str("FF 03 03 D1 FD 7B 07 A9 FD C3 01 91 F9 43 00 F9 F8 5F 09 A9 F6 57 0A A9 F4 4F 0B A9 59 D0 3B D5 F6 03 03 2A 28 17 40 F9 F5 03 02 AA F3 03 00 AA A8 83 1F F8 28 10 40 F9"),
];
#[cfg(target_arch = "arm")]
const RPMC_PATTERNS: [Pattern; 2] = [
    //1.21.120.4
    Pattern::from_str(
        "F0 B5 03 AF 2D E9 00 0F 8B B0 82 46 DF F8 ?? ?? 9B 46 91 46 78 44 00 68 00 68 0A 90",
    ),
    // V1.21.110-1.19.50
    Pattern::from_str(
        "F0 B5 03 AF 2D E9 00 ?? ?? B0 ?? 46 ?? 48 98 46 92 46 78 44 00 68 00 68 ?? 90 08 69",
    ),
];

#[cfg(target_arch = "x86_64")]
const RPMC_PATTERNS: [Pattern; 2] = [
    Pattern::from_str("55 41 57 41 56 41 55 41 54 53 48 83 EC ? 41 89 CF 49 89 D6 48 89 FB 64 48 8B 04 25 28 00 00 00 48 89 44 24 ? 48 8B 7E"),
    Pattern::from_str("55 41 57 41 56 53 48 83 EC ? 41 89 CF 49 89 D6 48 89 FB 64 48 8B 04 25 28 00 00 00 48 89 44 24 ? 48 8B 7E"),
];

pub fn startup() -> Option<BackendFn> {
    log::info!("Starting, mbl2 version v0.1.12");
    let Ok(mcmaps) = find_minecraft_library_manually() else {
        log::error!("Cannot find libminecraftpe.so in memory maps - device not supported");
        return None;
    };
    let Some(addr) = find_signatures(&RPMC_PATTERNS, &mcmaps) else {
        log::error!("No signature was found");
        return None;
    };
    log::info!("Hooking ResourcePackManager constructor");
    unsafe {
        rpm_ctor::hook_address(addr as *mut u8);
    };
    log::info!("Hooking AssetManager functions");
    Some(|name| {
        let mut resource_loc = ResourceLocation::new();
        let cpppath = resource_loc.get_path();
        cpppath.push_bytes(name.as_os_str().as_encoded_bytes());
        let aah = PACKM_OBJ.lock().ignore_poison();
        if let Some(yay) = aah.as_ref() {
            return Some(Ok(CowFile::Cxx(Cursor::new(
                yay.load_resource(resource_loc)?,
            ))));
        }
        None
    })
}
// A very minimal map range
#[derive(Debug)]
struct SimpleMapRange {
    start: usize,
    size: usize,
}

impl SimpleMapRange {
    /// Get the address where this range starts
    const fn start(&self) -> usize {
        self.start
    }

    /// Get the address where this range ends
    const fn size(&self) -> usize {
        self.size
    }
}

fn find_minecraft_library_manually() -> Result<Vec<SimpleMapRange>, Box<dyn std::error::Error>> {
    let contents = fs::read("/proc/self/maps")?;
    let mut ranges = Vec::new();
    for line in contents.lines().filter(|l| !l.trim_ascii().is_empty()) {
        // Not too pretty but this method prevents crashes
        let Some((addr_start, addr_end)) = parse_range(line) else {
            continue;
        };
        let start = usize::from_radix_16(addr_start).0;
        let end = usize::from_radix_16(addr_end).0;
        log::info!("Found libminecraftpe.so region at: {:x}-{:x}", start, end);
        ranges.push(SimpleMapRange {
            start,
            size: end - start,
        });
    }

    if ranges.is_empty() {
        Err("libminecraftpe.so not found in memory maps".into())
    } else {
        Ok(ranges)
    }
}
/// Separated into function due to option spam
fn parse_range(buf: &[u8]) -> Option<(&[u8], &[u8])> {
    let mut line = buf.split(|v| v.is_ascii_whitespace());
    let addr_range = line.next()?;
    let perms = line.next()?;
    let pathname = line.next_back()?;
    if perms.contains(&b'x') && pathname.ends_with(b"libminecraftpe.so") {
        return addr_range.split_once_str(b"-");
    }
    None
}

fn find_signatures(signatures: &[Pattern], ranges: &[SimpleMapRange]) -> Option<*const u8> {
    for sig in signatures {
        for range in ranges {
            let libbytes =
                unsafe { slice::from_raw_parts(range.start() as *const u8, range.size()) };
            let addr = sig.search(libbytes, tinypatscan::Algorithm::Simd);
            if let Some(val) = addr {
                let addr = unsafe { libbytes.as_ptr().byte_add(val) };
                #[cfg(target_arch = "arm")]
                let addr = unsafe { addr.offset(1) };
                log::info!(
                    "Found signature in region {:x}-{:x} at offset {:x}",
                    range.start(),
                    range.start() + range.size(),
                    val
                );
                return Some(addr);
            }
        }
        log::error!("Cannot find signature in any region");
    }
    None
}

// A resource pack manager object
pub static PACKM_OBJ: Mutex<Option<ResourcePackManager>> = Mutex::new(None);
// The resource pack manager load function
// pub static RPM_LOAD: OnceLock<RpmLoadFn> = OnceLock::new();

hook_fn! {
    fn rpm_ctor(this: *mut libc::c_void,unk1: usize,unk2: usize,needs_init: bool) -> *mut libc::c_void = {
        use crate::mbl::loader::ResourcePackManager;
        use crate::LockResultExt;
        log::info!("rpm ctor called");
        let result = call_original(this, unk1, unk2, needs_init);
        log::info!("RPM pointer has been obtained");
        *crate::mbl::PACKM_OBJ.lock().ignore_poison() = Some(ResourcePackManager::wrap(this));

        // Not doing this just adds overhead
        self_disable();
        log::info!("hook exit");
        result
    }
}
