//! The free space of the disk that holds Shinatama's data, sampled off the caller's thread. A
//! full disk stops the proxy and Shinatama from saving; the panel and the open errors name it.
use std::{
    path::{Component, Path, PathBuf, Prefix},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

/// Under this much free space the disk is named as low.
pub(crate) const LOW_BYTES: u64 = 1 << 30;
const UNKNOWN: u64 = u64::MAX;
const SAMPLE_EVERY: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiskLevel {
    Ok,
    Low,
    /// Under the database's save floor: typed writes are refused, and the proxy fails too.
    Full,
}
impl DiskLevel {
    /// The level as the status reply carries it.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Low => "low",
            Self::Full => "full",
        }
    }
}
pub(crate) fn level(free_bytes: u64) -> DiskLevel {
    if free_bytes < shinatama_state::maintenance::SAVE_FLOOR_BYTES {
        DiskLevel::Full
    } else if free_bytes < LOW_BYTES {
        DiskLevel::Low
    } else {
        DiskLevel::Ok
    }
}
/// The disk a path is on, as the user names it: `D:` for `D:\...` (also `\\?\D:\...`), otherwise
/// the path's first component.
pub(crate) fn root_of(path: &Path) -> String {
    match path.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => format!("{}:", letter as char),
            _ => prefix.as_os_str().to_string_lossy().into_owned(),
        },
        Some(component) => component.as_os_str().to_string_lossy().into_owned(),
        None => String::new(),
    }
}
/// What a low or full disk means, in the words the panel and the open errors use.
pub(crate) fn line(root: &str, free_bytes: u64) -> Option<String> {
    let megabytes = free_bytes / (1024 * 1024);
    match level(free_bytes) {
        DiskLevel::Full => Some(format!(
            "Disk {root} is full ({megabytes} MB free): saving fails for the proxy and for Shinatama."
        )),
        DiskLevel::Low => Some(format!("Disk {root} has {megabytes} MB free.")),
        DiskLevel::Ok => None,
    }
}
/// The last free-space reading of one directory's disk; unknown until the first sample and after
/// a failed one.
pub(crate) struct DiskWatch {
    directory: PathBuf,
    root: String,
    free: AtomicU64,
}
impl DiskWatch {
    pub(crate) fn unsampled(directory: PathBuf) -> Self {
        Self {
            root: root_of(&directory),
            directory,
            free: AtomicU64::new(UNKNOWN),
        }
    }
    /// Samples now and every 10 s on its own thread, for the life of the app.
    pub(crate) fn start(directory: PathBuf) -> Arc<Self> {
        let watch = Arc::new(Self::unsampled(directory));
        let sampler = Arc::clone(&watch);
        let _ = std::thread::Builder::new()
            .name("disk-space".into())
            .spawn(move || {
                loop {
                    sampler.sample();
                    std::thread::sleep(SAMPLE_EVERY);
                }
            });
        watch
    }
    fn sample(&self) {
        let free = shinatama_state::maintenance::free_bytes(&self.directory).unwrap_or(UNKNOWN);
        self.free.store(free, Ordering::Release);
    }
    pub(crate) fn free_bytes(&self) -> Option<u64> {
        let free = self.free.load(Ordering::Acquire);
        (free != UNKNOWN).then_some(free)
    }
    pub(crate) fn root(&self) -> &str {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    #[test]
    fn levels_follow_the_save_floor_and_one_gigabyte() {
        use shinatama_state::maintenance::SAVE_FLOOR_BYTES as F;
        assert_eq!(level(0), DiskLevel::Full);
        assert_eq!(level(F - 1), DiskLevel::Full);
        assert_eq!(level(F), DiskLevel::Low);
        assert_eq!(level(LOW_BYTES - 1), DiskLevel::Low);
        assert_eq!(level(LOW_BYTES), DiskLevel::Ok);
    }
    #[test]
    fn lines_name_the_disk_and_its_free_megabytes() {
        const MIB: u64 = 1024 * 1024;
        assert_eq!(root_of(Path::new(r"D:\Software\Shinatama\data")), "D:");
        assert_eq!(
            line("D:", 5 * MIB).as_deref(),
            Some("Disk D: is full (5 MB free): saving fails for the proxy and for Shinatama.")
        );
        assert_eq!(
            line("D:", 512 * MIB).as_deref(),
            Some("Disk D: has 512 MB free.")
        );
        assert_eq!(line("D:", 2 * LOW_BYTES), None);
    }
    #[test]
    fn a_watch_is_unknown_until_its_first_sample() {
        let watch = DiskWatch::unsampled(PathBuf::from(r"D:\x"));
        assert_eq!(watch.free_bytes(), None);
        assert_eq!(watch.root(), "D:");
    }
}
