use std::{
    borrow::Cow,
    ffi::OsStr,
    hash::Hash,
    ops::RangeFrom,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::Arc,
};


pub struct Resource<'a> {
    path: Cow<'a, Path>,
    resource_offset: RangeFrom<usize>,
    pack_uuid_hash: Option<Arc<String>>,
    is_archived: bool,
}

impl<'a> Resource<'a> {
    fn new_self(
        path: Cow<'a, Path>,
        res_offset: Option<RangeFrom<usize>>,
        pack_uuid_hash: Option<Arc<String>>,
        is_archived: bool,
    ) -> Self {
        Self {
            path,
            resource_offset: res_offset.unwrap_or(0..),
            pack_uuid_hash,
            is_archived,
        }
    }
    pub fn get_uuid(&self) -> Option<Arc<String>> {
        self.pack_uuid_hash.clone()
    }
    pub fn new_zip_resource(path: Cow<'a, Path>, uuid: Arc<String>) -> Self {
        Self::new_self(path, None, Some(uuid), true)
    }
    pub fn new_nameless(path: Cow<'a, Path>) -> Self {
        Self::new_self(path, None, None, false)
    }
    pub fn new(path: PathBuf, prefix: &Path, uuid: Arc<String>) -> Option<Self> {
        let strip = path.strip_prefix(prefix).ok()?;
        let bytes = path.as_os_str().as_encoded_bytes();
        let range = range_start_of(bytes, strip.as_os_str().as_bytes())?;
        Some(Self::new_self(
            Cow::Owned(path),
            Some(range),
            Some(uuid),
            false,
        ))
    }
    /// Will return None if the resource isnt actually a file
    pub fn path(&self) -> Option<&Path> {
        if self.is_archived {
            return None;
        }
        Some(self.path.as_ref())
    }
    pub fn resource_name(&self) -> &Path {
        let osbytes = self.path.as_os_str().as_bytes();
        let resource = &osbytes[self.resource_offset.clone()];
        let osstr = OsStr::from_bytes(resource);
        Path::new(osstr)
    }
}
impl<'a> Hash for Resource<'a> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let osbytes = self.path.as_os_str().as_encoded_bytes();
        let resource = &osbytes[self.resource_offset.clone()];
        resource.hash(state);
    }
}
// Spoiler: This is Bullshit
impl<'a> PartialEq for Resource<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.resource_name() == other.resource_name()
    }
}
impl<'a> Eq for Resource<'a> {}

fn wrapping_sub_ptr<T>(lhs: *const T, rhs: *const T) -> usize {
    let pointee_size = std::mem::size_of::<T>();
    (lhs as usize - rhs as usize) / pointee_size
}
/// Get the starting range at which `inner` belongs inside `outer`
pub fn range_start_of<T>(outer: &[T], inner: &[T]) -> Option<RangeFrom<usize>> {
    let outer = outer.as_ptr_range();
    let inner = inner.as_ptr_range();
    if outer.start <= inner.start {
        Some(wrapping_sub_ptr(inner.start, outer.start)..)
    } else {
        None
    }
}
