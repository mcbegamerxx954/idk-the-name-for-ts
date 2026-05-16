#![allow(non_snake_case)]

// #[cfg(feature = "autofixing")]
// use crate::autofixer::OPTS;
// use crate::LockResultExt;
// use jni::{
//     objects::{JObject, JObjectArray, JString},
//     sys::{jboolean, jint, JNI_TRUE, JNI_VERSION_1_6},
//     JNIEnv, JavaVM, NativeMethod,
// };
// #[cfg(feature = "autofixing")]
// use materialbin::MinecraftVersion;
// use std::os::raw::c_void;

// macro_rules! native_method {
//     ($name:ident, $sig:literal) => {
//         NativeMethod::new(stringify!($name), $sig, $name as *mut c_void)
//     };
// }
// #[no_mangle]
// extern "C" fn JNI_OnLoad(vm: JavaVM, _: c_void) -> jint {
//     if let Err(e) = jni_start(&vm) {
//         log::error!("Error in jni_onload: {e}");
//     }
//     JNI_VERSION_1_6
// }
// fn jni_start(vm: &JavaVM) -> jni::errors::Result<()> {
//     let mut env = vm.attach_current_thread(|env| {
//     let clazz = env.find_class("io/bambosan/mbloader/launcherUtils/LibBindings")?;
//     #[cfg(feature = "autofixing")]
//     let mets = [
//         native_method!(setAutofixVersions, "([Ljava/lang/String;)V"),
//         native_method!(setLightmapAutofixer, "(Z)V"),
//         native_method!(setTextureLodAutofixer, "(Z)V"),
//     ];
//     #[cfg(feature = "autofixing")]
//     env.register_native_methods(clazz, &mets)?;
//     });
//     Ok(())
// }
// #[cfg(feature = "autofixing")]
// extern "C" fn setAutofixVersions(mut env: JNIEnv, _thiz: JObject, versions: JObjectArray) {
//     if let Err(e) = setAutofixVersions_rel(&mut env, versions) {
//         log::error!("Jni error: {e}");
//     }
// }
// #[cfg(feature = "autofixing")]
// fn setAutofixVersions_rel(env: &mut JNIEnv, versions: JObjectArray) -> jni::errors::Result<()> {
//     let length = env.get_array_length(&versions)?;
//     let mut rs_versions = Vec::new();
//     for index in 0..length {
//         let string: JString = env.get_object_array_element(&versions, index)?.into();
//         let sus = env.get_string(&string)?;
//         let version = version_from_str(sus.to_bytes()).expect("String is not a mtbin version");
//         rs_versions.push(version);
//     }
//     let mut opts = OPTS.lock().ignore_poison();
//     opts.autofixer_versions = rs_versions;
//     Ok(())
// }
// #[cfg(feature = "autofixing")]
// fn version_from_str(string: &[u8]) -> Option<MinecraftVersion> {
//     let mcversion = match string {
//         b"v1.18.30" => MinecraftVersion::V1_18_30,
//         b"v1.19.60" => MinecraftVersion::V1_19_60,
//         b"v1.20.80" => MinecraftVersion::V1_20_80,
//         b"v1.21.20" => MinecraftVersion::V1_21_20,
//         b"v1.21.110+" => MinecraftVersion::V1_21_110,
//         _ => return None,
//     };
//     Some(mcversion)
// }
// #[cfg(feature = "autofixing")]
// extern "C" fn setLightmapAutofixer(_env: JNIEnv, _thiz: JObject, on: jboolean) {
//     let mut opts = OPTS.lock().ignore_poison();
//     opts.handle_lightmaps = to_bool(on);
// }
// #[cfg(feature = "autofixing")]
// extern "C" fn setTextureLodAutofixer(_env: JNIEnv, _thiz: JObject, on: jboolean) {
//     let mut opts = OPTS.lock().ignore_poison();
//     opts.handle_texturelods = to_bool(on);
// }
// pub trait NativeMethodCtor {
//     fn new(name: &str, sig: &str, fn_ptr: *mut c_void) -> Self;
// }
// impl NativeMethodCtor for NativeMethod {
//     fn new(name: &str, sig: &str, fn_ptr: *mut c_void) -> Self {
//         NativeMethod {
//             name: name.into(),
//             sig: sig.into(),
//             fn_ptr,
//         }
//     }
// }
// const fn to_bool(jni_bool: jboolean) -> bool {
//     jni_bool == JNI_TRUE
// }
