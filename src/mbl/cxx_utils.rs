// use cxx::CxxString;
use std::{
    ffi::c_void,
    mem::{transmute, MaybeUninit},
    pin::Pin,
};
// Smart pointer for ResourceLocation
#[repr(transparent)]
pub struct ResourceLocation(*mut c_void);
impl Default for ResourceLocation {
    fn default() -> Self {
        Self::new()
    }
}
#[repr(C)]
pub struct CxxString {
    _private: [u8; 0],
    _pinned: core::marker::PhantomData<core::marker::PhantomPinned>,
}
impl CxxString {
    pub fn as_ptr(&self) -> *const u8 {
        unsafe { string_data(self) }
    }
    pub fn len(&self) -> usize {
        unsafe { string_length(self) }
    }
    pub fn as_bytes(&self) -> &[u8] {
        let data = self.as_ptr();
        let len = self.len();
        unsafe { core::slice::from_raw_parts(data, len) }
    }
    pub fn clear(self: Pin<&mut Self>) {
        unsafe { string_clear(self) }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn push_bytes(self: Pin<&mut Self>, bytes: &[u8]) {
        unsafe { string_push(self, bytes.as_ptr(), bytes.len()) }
    }

    pub fn reserve(self: Pin<&mut Self>, additional: usize) {
        let new_cap = self
            .len()
            .checked_add(additional)
            .expect("CxxString capacity overflow");
        unsafe { string_reserve_total(self, new_cap) }
    }
}

extern "C" {
    #[link_name = "cxx_string$init"]
    fn string_init(this: &mut MaybeUninit<CxxString>, ptr: *const u8, len: usize);
    #[link_name = "cxx_string$destroy"]
    fn string_destroy(this: &mut MaybeUninit<CxxString>);
    #[link_name = "cxx_string$reserve_total"]
    fn string_reserve_total(this: Pin<&mut CxxString>, new_cap: usize);
    #[link_name = "cxx_string$clear"]
    fn string_clear(this: Pin<&mut CxxString>);
    #[link_name = "cxx_string$length"]
    fn string_length(this: &CxxString) -> usize;
    #[link_name = "cxx_string$data"]
    fn string_data(this: &CxxString) -> *const u8;
    #[link_name = "cxx_string$push"]
    fn string_push(this: Pin<&mut CxxString>, ptr: *const u8, len: usize);
}
impl ResourceLocation {
    pub fn new() -> Self {
        unsafe { resource_location_init() }
    }
    pub fn get_path<'a>(&mut self) -> Pin<&'a mut CxxString> {
        // SAFETY: We just did not force it to be pin since then borrow checker gets angry
        unsafe {
            let ptr = resource_location_path(self.0);
            transmute(ptr)
        }
    }
}
impl Drop for ResourceLocation {
    fn drop(&mut self) {
        // SAFETY: We handle the scope so its good
        unsafe { resource_location_free(self.0) }
    }
}
// Linking against string.cpp
extern "C" {
    fn resource_location_init() -> ResourceLocation;
    fn resource_location_path(loc: *mut libc::c_void) -> *mut CxxString;
    fn resource_location_free(loc: *mut libc::c_void);
}
#[repr(C)]
pub struct StackString {
    // Static assertions in cxx.cc validate that this is large enough and
    // aligned enough.
    space: MaybeUninit<[usize; 8]>,
}
impl AsRef<[u8]> for StackString {
    fn as_ref(&self) -> &[u8] {
        unsafe {
            let this = &*self.space.as_ptr().cast::<MaybeUninit<CxxString>>();
            let cxxptr = &*this.as_ptr();
            cxxptr.as_bytes()
        }
    }
}
#[allow(missing_docs)]
impl StackString {
    pub const fn new() -> Self {
        Self {
            space: MaybeUninit::uninit(),
        }
    }

    pub unsafe fn init(&mut self, value: impl AsRef<[u8]>) -> Pin<&mut CxxString> {
        let value = value.as_ref();
        unsafe {
            let this = &mut *self.space.as_mut_ptr().cast::<MaybeUninit<CxxString>>();
            string_init(this, value.as_ptr(), value.len());
            Pin::new_unchecked(&mut *this.as_mut_ptr())
        }
    }
}

impl Drop for StackString {
    fn drop(&mut self) {
        unsafe {
            let this = &mut *self.space.as_mut_ptr().cast::<MaybeUninit<CxxString>>();
            string_destroy(this);
        }
    }
}
impl std::io::Write for Pin<&mut CxxString> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.as_mut().push_bytes(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
