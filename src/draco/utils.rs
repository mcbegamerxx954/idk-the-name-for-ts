use super::errors::{DataError, PackParseError};
use crate::draco::resource::{Resource, ZipsContainer};
use crate::opt_path_join;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs::File;
use std::hash::DefaultHasher;
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use struson::json_path;
use struson::reader::{JsonReader, JsonStreamReader, ReaderSettings};
const JSON_SETTINGS: ReaderSettings = ReaderSettings {
    allow_comments: true,
    allow_multiple_top_level: true,
    allow_trailing_comma: true,
    track_path: false,
    max_nesting_depth: Some(5),
    restrict_number_values: false,
};
// use tinyjson::{JsonParseError, JsonParser, JsonValue};
use walkdir::DirEntry;
use zip::read::ZipFile;
use zip::ZipArchive;
// Keeps track and manages data about the minecraft Resource Pack Structure
#[derive(Debug)]
pub struct DataManager {
    valid_packs: Vec<ValidPack>,
    active_packs: Vec<GlobalPack>,
    pub resourcepacks_dir: PathBuf,
    pub active_packs_path: PathBuf,
}
enum PackType {
    Uncompressed(PathBuf),
    Compressed(PathBuf, ZipArchive<File>),
}
// A pack that minecraft verified as valid
#[derive(Debug)]
pub struct ValidPack {
    uuid: String,
    path: PathBuf,
    version: Vec<u32>,
    zip: Option<ZipArchive<File>>,
}

impl ValidPack {
    // We do not use serde because it is much more strict
    // than bedrock in terms of json parsing
    fn parse_manifest<T: Read>(
        manifest_reader: &mut T,
        mut pack_path: PathBuf,
    ) -> Result<Self, PackParseError> {
        // let manifest = File::open(&pack_path)?;
        let mut json = JsonStreamReader::new_custom(manifest_reader, JSON_SETTINGS);
        json.seek_to(&json_path!["header"])?;
        json.begin_object()?;
        let mut uuid = None;
        let mut version = None;
        while json.has_next()? {
            match json.next_name()? {
                "uuid" => uuid = Some(json.next_string()?),
                "version" => version = Some(version_parse(&mut json)?),
                _ => json.skip_value()?,
            }
        }
        json.end_object()?;
        let uuid = uuid.ok_or(PackParseError::InvalidManifest("uuid"))?;
        let version = version.ok_or(PackParseError::InvalidManifest("version"))?;
        // We assume that this had a "manifest.json" component, so we pop it to have it as a regular path
        pack_path.pop();
        Ok(Self {
            uuid,
            path: pack_path,
            version,
            zip: None,
        })
    }
    fn set_zip(&mut self, zip: ZipArchive<File>) {
        self.zip = Some(zip);
    }
    fn handle_zip_resource(&mut self, resource: &Resource) -> Option<Vec<u8>> {
        let sus = resource.resource_name();
        let mut file = self.zip.as_mut()?.by_path(sus).ok()?;
        let mut buf = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut buf).ok()?;
        Some(buf)
    }

    fn impl_get_files(&self, path: &Path, set: &mut HashSet<Resource>, uuid: Arc<String>) {
        match self.zip {
            Some(ref zip) => get_files_archive(zip, set, uuid),
            None => get_files(path, set, uuid),
        }
    }
    pub fn get_pack_files(&self, subpack: Option<&str>, set: &mut HashSet<Resource>) {
        let uuid_tag = Arc::new(self.uuid.clone());
        // We add the subpack first as it has priority over main pack
        if let Some(subpack) = subpack {
            let mut buffer = [0_u8; 128];
            let joined = opt_path_join(&mut buffer, &["/subpacks/", &subpack]);
            self.impl_get_files(&joined, set, uuid_tag.clone());
        }
        // Any files that the subpack has will override these
        self.impl_get_files(&self.path, set, uuid_tag);
    }
}

fn get_files(path: &Path, file_list: &mut HashSet<Resource>, uuid: Arc<String>) {
    let walker = walkdir::WalkDir::new(path);
    let iter = walker.into_iter().filter_entry(is_interesting).flatten();
    for file_path in iter.map(|e| e.into_path()) {
        let Some(resource_path) = Resource::new(file_path, path, uuid.clone()) else {
            continue;
        };
        file_list.insert(resource_path);
    }
}

fn get_files_archive<T: Read + Seek>(
    zip: &ZipArchive<T>,
    // subpack: Option<String>,
    set: &mut HashSet<Resource>,
    uuid: Arc<String>,
) {
    const ALLOWED_PATHS: [&str; 4] = ["renderer", "vanilla_cameras", "hbui", "custom_persona"];
    let check_path = |e: &Path| ALLOWED_PATHS.into_iter().any(|a| e.starts_with(a));
    for name in zip.file_names() {
        let path = Path::new(OsStr::new(name));
        if check_path(path) {
            let resource = Resource::new_zip_resource(Cow::Owned(path.to_path_buf()), uuid.clone());
            set.insert(resource);
        }
        // let path = Path::new(OsStr::new(name));
        // let mut components = path.iter();
        // let root = components.next().unwrap();
        // if let Some(ref subpack) = subpack {
        //     if root == subpack.as_str() && check_path(components.as_path()) {
        //         let final_path = components.as_path().to_path_buf();
        //         let resource = Resource::new_zip_resource(final_path.into(), uuid.clone());
        //         set.insert(resource);
        //     }
        // }
        // if check_path(path) {
        //     let resource = Resource::new_zip_resource(Cow::Owned(path.to_owned()), uuid.clone());
        //     set.insert(resource);
        // }
    }
}
fn is_interesting(entry: &DirEntry) -> bool {
    const ALLOWED_PATHS: [&str; 4] = ["renderer", "vanilla_cameras", "hbui", "custom_persona"];
    if entry.depth() == 1 {
        let folder = entry.file_name();
        // By doing comparison we delegate black magic to rust std
        ALLOWED_PATHS.into_iter().any(|p| p == folder)
    } else {
        true
    }
}
// A active global pack
#[derive(Debug)]
struct GlobalPack {
    pack_id: String,
    subpack: Option<String>,
    version: Vec<u32>,
}
impl GlobalPack {
    fn parse(path: &Path) -> Result<Vec<Self>, DataError> {
        let manifest = File::open(path)?;
        let mut json = JsonStreamReader::new_custom(manifest, JSON_SETTINGS);
        json.begin_array()?;
        let mut global_packs = Vec::new();
        while json.has_next()? {
            json.begin_object()?;
            global_packs.push(Self::parse_one(&mut json)?);
            json.end_object()?;
        }
        json.end_array()?;
        Ok(global_packs)
    }
    fn parse_one(json: &mut impl JsonReader) -> Result<Self, DataError> {
        let mut pack_id = None;
        let mut subpack = None;
        let mut version = None;
        while json.has_next()? {
            match json.next_name()? {
                "pack_id" => pack_id = Some(json.next_string()?),
                "subpack" => subpack = Some(json.next_string()?),
                "version" => version = Some(version_parse(json)?),
                _ => json.skip_value()?,
            }
        }
        let pack_id = pack_id.ok_or(DataError::InvalidData("id"))?;
        let version = version.ok_or(DataError::InvalidData("version"))?;
        Ok(Self {
            pack_id,
            subpack,
            version,
        })
    }
}

impl DataManager {
    /// Made solely for my convenience, generates a very useless dataman
    pub const fn empty() -> Self {
        Self::init_data(PathBuf::new(), PathBuf::new())
    }
    // Get minecraft paths and create itself
    pub const fn init_data(json_path: PathBuf, resourcepacks_path: PathBuf) -> Self {
        Self {
            active_packs: Vec::new(),
            valid_packs: Vec::new(),
            resourcepacks_dir: resourcepacks_path,
            active_packs_path: json_path,
        }
    }

    // Get a list of shader paths
    pub fn shader_paths<'a>(&mut self, list: &mut HashSet<Resource>) -> Result<(), DataError> {
        let global_packs: Vec<GlobalPack> = GlobalPack::parse(&self.active_packs_path)?;
        log::debug!("global_packs parsed: {:#?}", global_packs);
        let packs = self.get_installed_packs()?;
        log::debug!("Installed packs: {packs:#?}");
        // let mut final_paths = HashSet::new();
        // Explanation: we use .rev to reverse the iterator since this way we can avoid
        // some checks
        for pack in global_packs.iter().rev() {
            if let Some(vp) = packs.iter().find(|vp| matches_pack(&pack, vp)) {
                // We pass the hashset directly to avoid useless allocations that get dropped instantly
                vp.get_pack_files(pack.subpack.as_deref(), list);
            }
        }
        self.valid_packs = packs;
        self.active_packs = global_packs;
        Ok(())
    }
    pub fn read_resource(&mut self, resource: &Resource) -> Option<Vec<u8>> {
        let resource_uuid = resource.get_uuid()?;
        let pack = self
            .valid_packs
            .iter_mut()
            .find(|e| e.uuid == *resource_uuid)?;
        pack.handle_zip_resource(resource)
    }
    fn get_installed_packs(&self) -> Result<Vec<ValidPack>, DataError> {
        let pack_dirs = std::fs::read_dir(&self.resourcepacks_dir)?;
        let mut packs = Vec::new();
        for dir in pack_dirs.flatten() {
            let path = dir.path();
            if path.extension().is_some_and(|a| a == "mcpack") {
                let zipfile = File::open(&path).unwrap();
                let mut zip = ZipArchive::new(zipfile).unwrap();
                let mut manifest = zip.by_name("manifest.json").unwrap();
                let mut validpack = match ValidPack::parse_manifest(&mut manifest, path) {
                    Ok(path) => path,
                    Err(e) => {
                        log::warn!("FUck");
                        continue;
                    }
                };
                drop(manifest);
                validpack.set_zip(zip);
                packs.push(validpack);
                continue;
            }
            if !dir.file_type()?.is_dir() {
                continue;
            }
            let Some(manifest_path) = find_pack_manifest(&path) else {
                log::warn!("Cannot find pack manifest for dir: {:?}", path);
                continue;
            };
            let mut fileyay = match File::open(&manifest_path) {
                Ok(yay) => yay,
                Err(e) => {
                    log::warn!("couldnt open manifest file");
                    continue;
                }
            };
            let validpack = match ValidPack::parse_manifest(&mut fileyay, manifest_path) {
                Ok(pack) => pack,
                Err(err) => {
                    log::error!("Pack manifest parse failed: {err}");
                    continue;
                }
            };
            packs.push(validpack);
        }
        Ok(packs)
    }
}
fn matches_pack(global_pack: &GlobalPack, valid_pack: &ValidPack) -> bool {
    valid_pack.uuid.eq_ignore_ascii_case(&global_pack.pack_id)
        && valid_pack.version == global_pack.version
}

// This is rare, but can happen
fn find_pack_manifest(path: &Path) -> Option<PathBuf> {
    let walker = walkdir::WalkDir::new(path).sort_by(compare);
    walker
        .into_iter()
        .flatten()
        .find(|entry| entry.file_name() == "manifest.json" && entry.file_type().is_file())
        .map(|e| e.into_path())
}
fn compare(entry1: &DirEntry, entry2: &DirEntry) -> Ordering {
    //    entry1.into_path()
    let ftype1 = entry1.file_type();
    let ftype2 = entry2.file_type();
    match (ftype1.is_file(), ftype2.is_file()) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => Ordering::Equal,
        (false, false) => Ordering::Equal,
    }
}
fn version_parse<T: JsonReader>(json: &mut T) -> Result<Vec<u32>, PackParseError> {
    let mut numbers = Vec::new();
    json.begin_array()?;
    while json.has_next()? {
        let workaround = json.next_number()?;
        numbers.push(workaround?);
    }
    json.end_array()?;
    Ok(numbers)
}
