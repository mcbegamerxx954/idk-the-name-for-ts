use crate::LockResultExt;

use super::errors::DataError;
use super::storage::StorageLocation;
use super::utils::DataManager;
use super::SHADER_PATHS;
use super::{get_storage_location, get_storage_path};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant};
pub static mut DATA_MANAGER: Mutex<DataManager> = Mutex::new(DataManager::empty());
pub static SHOULD_STOP: AtomicBool = AtomicBool::new(false);
pub fn setup_json_watcher(path: PathBuf) {
    let options_path = path.join("options.txt");
    let current_location = get_storage_location(&options_path).unwrap_or(StorageLocation::Internal);
    let path = get_storage_path(current_location);
    let mut data_manager = setup_dataman(&path);
    let resource_packs_dir = &mut data_manager.active_packs_path;
    // TODO: Rewrite this shit
    if !resource_packs_dir.exists() {
        let default_path = get_storage_path(StorageLocation::Internal);
        *resource_packs_dir = setup_dataman(&default_path).active_packs_path;
        if !resource_packs_dir.exists() {
            log::info!("no active_packs file found, using internal and hoping for the best");
        }
        log::info!("global packs json not found, defaulting to internal storage");
    }
    unsafe {
        *DATA_MANAGER.lock().ignore_poison() = data_manager;
    }

    let mut data_manager = unsafe { DATA_MANAGER.lock().ignore_poison() };
    startup_load(&mut data_manager);
    let (sender, reciever) = std::sync::mpsc::channel();
    let mut watcher = RecommendedWatcher::new(sender, Config::default()).unwrap();
    loop {
        if data_manager.active_packs_path.exists() {
            break;
        } else {
            std::thread::sleep(Duration::from_secs(5));
        }
    }
    watcher
        .watch(&data_manager.active_packs_path, RecursiveMode::NonRecursive)
        .unwrap();
    drop(data_manager);
    for event in reciever {
        let should_stop = SHOULD_STOP.load(Ordering::Acquire);
        if should_stop {
            // Something happened that requires us to stop this thread
            return;
        }
        // Recieve a filesystem event
        let event = match event {
            Ok(event) => event,
            Err(e) => {
                log::info!("Skipping event error: {e}");
                continue;
            }
        };
        log::debug!("Recieved interesting event: {:#?}", event);
        // Get the first filename in the event
        let Some(path) = event.paths.first() else {
            log::warn!("No event path found");
            continue;
        };
        let Some(file_name) = path.file_name() else {
            log::warn!("Event path has no filename");
            continue;
        };

        // if &data_manager.active_packs_path != path {
        //     log::warn!("Wrong path detected, correcting..");

        //     let mut data_manager = DATA_MANAGER.get_mut().ignore_poison();

        //     let new_dataman =
        //         DataManager::init_data(path.clone(), data_manager.resourcepacks_dir.clone());
        //     *data_manager = new_dataman;
        // }
        // This means that Minecraft has changed or read the resource list, let's do it too
        if file_name == "global_resource_packs.json" && event.kind.is_modify() {
            log::info!("Active rpacks changed, updating..");

            let mut data_manager = unsafe { DATA_MANAGER.get_mut().ignore_poison() };
            if let Err(e) = update_global_sp(&mut data_manager) {
                log::warn!("Updating shader paths failed: {e}");
            };
        }
    }
}
fn update_global_sp(dataman: &mut DataManager) -> Result<(), DataError> {
    let time = Instant::now();
    let mut locked_sp = SHADER_PATHS.lock().ignore_poison();
    locked_sp.clear();
    dataman.shader_paths(&mut locked_sp)?;
    log::info!(
        "Updated global shader paths in {}ms...",
        time.elapsed().as_millis()
    );
    Ok(())
}
fn startup_load(dataman: &mut DataManager) {
    log::info!("Trying to load files eagerly");
    if let Err(e) = update_global_sp(dataman) {
        log::error!("Damn we failed epically: {e}");
    };
}
fn setup_dataman(mc_path: &Path) -> DataManager {
    let mut json_path = mc_path.to_path_buf();
    json_path.extend([
        "games",
        "com.mojang",
        "minecraftpe",
        "global_resource_packs.json",
    ]);
    let mut resourcepacks_path = mc_path.to_path_buf();
    resourcepacks_path.extend(["games", "com.mojang", "resource_packs"]);
    DataManager::init_data(json_path, resourcepacks_path)
}
