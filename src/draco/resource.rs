use std::{
    borrow::Cow,
    ffi::OsStr,
    hash::Hash,
    ops::RangeFrom,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

pub struct Resource<'a> {
    path: Cow<'a, Path>,
    resource_start: RangeFrom<usize>,
}
impl<'a> Resource<'a> {
    pub fn new_nameless(path: Cow<'a, Path>) -> Self {
        Self {
            path,
            resource_start: 0..,
        }
    }
    pub fn new(path: PathBuf, prefix: &Path) -> Option<Self> {
        let strip = path.strip_prefix(prefix).ok()?;
        let bytes = path.as_os_str().as_encoded_bytes();
        let range = range_start_of(bytes, strip.as_os_str().as_bytes())?;
        Some(Self {
            path: Cow::Owned(path),
            resource_start: range,
        })
    }
    pub fn path(&self) -> &Path {
        self.path.as_ref()
    }
    pub fn resource_name(&self) -> &Path {
        let osbytes = self.path.as_os_str().as_bytes();
        let resource = &osbytes[self.resource_start.clone()];
        let osstr = OsStr::from_bytes(resource);
        Path::new(osstr)
    }
}
impl<'a> Hash for Resource<'a> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let osbytes = self.path.as_os_str().as_encoded_bytes();
        let resource = &osbytes[self.resource_start.clone()];
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
