use super::errors::{DataError, PackParseError};
use crate::draco::resource::Resource;
use crate::opt_path_join;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs::File;
use std::path::{Path, PathBuf};
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
// Keeps track and manages data about the minecraft Resource Pack Structure
#[derive(Debug)]
pub struct DataManager {
    pub resourcepacks_dir: PathBuf,
    pub active_packs_path: PathBuf,
}

// A pack that minecraft verified as valid
#[derive(Debug)]
pub struct ValidPack {
    uuid: String,
    path: PathBuf,
    version: Vec<u32>,
}

impl ValidPack {
    // We do not use serde because it is much more strict
    // than bedrock in terms of json parsing
    fn parse_manifest(mut pack_path: PathBuf) -> Result<Self, PackParseError> {
        let manifest = File::open(&pack_path)?;
        let mut json = JsonStreamReader::new_custom(manifest, JSON_SETTINGS);
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
        })
    }
    pub fn get_pack_files(&self, subpack: Option<String>, set: &mut HashSet<Resource>) {
        // We add the subpack first as it has priority over main pack
        if let Some(subpack) = subpack {
            let mut buffer = [0_u8; 128];
            let joined = opt_path_join(&mut buffer, &["/subpacks/", &subpack]);
            get_files(&joined, set);
        }
        // Any files that the subpack has will override these
        get_files(&self.path, set);
    }
}

fn get_files(path: &Path, file_list: &mut HashSet<Resource>) {
    let walker = walkdir::WalkDir::new(path);
    let iter = walker.into_iter().filter_entry(is_interesting).flatten();
    for file_path in iter.map(|e| e.into_path()) {
        let Some(resource_path) = Resource::new(file_path, path) else {
            continue;
        };
        file_list.insert(resource_path);
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
    // Get minecraft paths and create itself
    pub const fn init_data(json_path: PathBuf, resourcepacks_path: PathBuf) -> Self {
        Self {
            resourcepacks_dir: resourcepacks_path,
            active_packs_path: json_path,
        }
    }

    // Get a list of shader paths
    pub fn shader_paths<'a>(&self, list: &mut HashSet<Resource>) -> Result<(), DataError> {
        let global_packs: Vec<GlobalPack> = GlobalPack::parse(&self.active_packs_path)?;
        log::debug!("global_packs parsed: {:#?}", global_packs);
        let packs = self.get_installed_packs()?;
        log::debug!("Installed packs: {packs:#?}");
        // let mut final_paths = HashSet::new();
        // Explanation: we use .rev to reverse the iterator since this way we can avoid
        // some checks
        for pack in global_packs.into_iter().rev() {
            if let Some(vp) = packs.iter().find(|vp| matches_pack(&pack, vp)) {
                // We pass the hashset directly to avoid useless allocations that get dropped instantly
                vp.get_pack_files(pack.subpack, list);
            }
        }
        Ok(())
    }
    fn get_installed_packs(&self) -> Result<Vec<ValidPack>, DataError> {
        let pack_dirs = std::fs::read_dir(&self.resourcepacks_dir)?;
        let mut packs = Vec::new();
        for dir in pack_dirs.flatten() {
            if !dir.file_type()?.is_dir() {
                continue;
            }
            let Some(manifest_path) = find_pack_manifest(&dir.path()) else {
                log::warn!("Cannot find pack manifest for dir: {:?}", dir.path());
                continue;
            };
            let validpack = match ValidPack::parse_manifest(manifest_path) {
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
