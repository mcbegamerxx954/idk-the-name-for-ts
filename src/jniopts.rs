#![allow(non_snake_case)]
use jni::{
    objects::{JObject, JObjectArray, JString},
    sys::{jboolean, jint, JNI_TRUE, JNI_VERSION_1_6},
    JNIEnv, JavaVM, NativeMethod,
};
use materialbin::{MinecraftVersion, ALL_VERSIONS};
use std::{
    os::raw::c_void,
    sync::{LazyLock, Mutex},
};

use crate::LockResultExt;
pub struct Options {
    pub handle_lightmaps: bool,
    pub handle_texturelods: bool,
    pub autofixer_versions: Vec<MinecraftVersion>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            handle_lightmaps: true,
            handle_texturelods: true,
            autofixer_versions: ALL_VERSIONS.to_vec(),
        }
    }
}
macro_rules! native_method {
    ($name:ident, $sig:literal) => {
        NativeMethod::new(stringify!($name), $sig, $name as *mut c_void)
    };
}
#[no_mangle]
extern "C" fn JNI_OnLoad(vm: JavaVM, _: c_void) -> jint {
    let mut env = vm.get_env().unwrap();
    let clazz = env
        .find_class("io/bambosan/mbloader/launcherUtils/LibBindings")
        .unwrap();
    let mets = [
        native_method!(setAutofixVersions, "([Ljava/lang/String;)V"),
        native_method!(setLightmapAutofixer, "(Z)V"),
        native_method!(setTextureLodAutofixer, "(Z)V"),
    ];
    env.register_native_methods(clazz, &mets).unwrap();
    JNI_VERSION_1_6
}
pub static OPTS: LazyLock<Mutex<Options>> = LazyLock::new(|| Mutex::new(Options::default()));
extern "C" fn setAutofixVersions(mut env: JNIEnv, _thiz: JObject, versions: JObjectArray) {
    if let Err(e) = setAutofixVersions_rel(&mut env, versions) {
        log::error!("Jni error: {e}");
    }
}
fn setAutofixVersions_rel(env: &mut JNIEnv, versions: JObjectArray) -> jni::errors::Result<()> {
    let length = env.get_array_length(&versions)?;
    let mut rs_versions = Vec::new();
    for index in 0..length {
        let string: JString = env.get_object_array_element(&versions, index)?.into();
        let sus = env.get_string(&string)?;
        let version = version_from_str(sus.to_bytes()).expect("String is not a mtbin version");
        rs_versions.push(version);
    }
    let mut opts = OPTS.lock().ignore_poison();
    opts.autofixer_versions = rs_versions;
    Ok(())
}

fn version_from_str(string: &[u8]) -> Option<MinecraftVersion> {
    let mcversion = match string {
        b"v1.18.30" => MinecraftVersion::V1_18_30,
        b"v1.19.60" => MinecraftVersion::V1_19_60,
        b"v1.20.80" => MinecraftVersion::V1_20_80,
        b"v1.21.20" => MinecraftVersion::V1_21_20,
        b"v1.21.110+" => MinecraftVersion::V1_21_110,
        _ => return None,
    };
    Some(mcversion)
}
#[no_mangle]
extern "C" fn setLightmapAutofixer(_env: JNIEnv, _thiz: JObject, on: jboolean) {
    let mut opts = OPTS.lock().ignore_poison();
    opts.handle_lightmaps = to_bool(on);
}
#[no_mangle]
extern "C" fn setTextureLodAutofixer(_env: JNIEnv, _thiz: JObject, on: jboolean) {
    let mut opts = OPTS.lock().ignore_poison();
    opts.handle_texturelods = to_bool(on);
}
pub trait NativeMethodCtor {
    fn new(name: &str, sig: &str, fn_ptr: *mut c_void) -> NativeMethod {
        NativeMethod {
            name: name.into(),
            sig: sig.into(),
            fn_ptr,
        }
    }
}
impl NativeMethodCtor for NativeMethod {}
fn to_bool(jni_bool: jboolean) -> bool {
    jni_bool == JNI_TRUE
}
