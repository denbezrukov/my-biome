use biome_fs::{
    File, FileSystem, FileSystemDiagnostic, FsErrorKind, OpenOptions, PathKind, TraversalScope,
};
use biome_resolver::FsWithResolverProxy;
use camino::{Utf8Path, Utf8PathBuf};
use parking_lot::Mutex;
use std::{collections::BTreeMap, io, sync::Arc};

type PathResult = Result<Utf8PathBuf, (io::ErrorKind, String)>;

/// Every parser in an acquisition sees the same bytes, including editor overlays.
pub(super) struct PinnedFs {
    base: Arc<dyn FsWithResolverProxy>,
    pub overlays: BTreeMap<Utf8PathBuf, String>,
    reads: Mutex<BTreeMap<Utf8PathBuf, Result<String, String>>>,
    kinds: Mutex<BTreeMap<(Utf8PathBuf, bool), Result<PathKind, String>>>,
    identities: Mutex<BTreeMap<Utf8PathBuf, PathResult>>,
    links: Mutex<BTreeMap<Utf8PathBuf, PathResult>>,
}
impl std::panic::RefUnwindSafe for PinnedFs {}
impl PinnedFs {
    pub fn new(
        base: Arc<dyn FsWithResolverProxy>,
        overlays: BTreeMap<Utf8PathBuf, String>,
    ) -> Self {
        Self {
            base,
            overlays,
            reads: Mutex::default(),
            kinds: Mutex::default(),
            identities: Mutex::default(),
            links: Mutex::default(),
        }
    }
    fn error(path: &Utf8Path) -> FileSystemDiagnostic {
        FileSystemDiagnostic {
            path: path.to_string(),
            severity: biome_diagnostics::Severity::Error,
            error_kind: FsErrorKind::CantReadFile,
            source: None,
        }
    }
    fn live_kind(&self, path: &Utf8Path, symlink: bool) -> Result<PathKind, String> {
        if self.overlays.contains_key(path) {
            return Ok(PathKind::File { is_symlink: false });
        }
        if self
            .overlays
            .keys()
            .any(|overlay| overlay.starts_with(path))
        {
            return Ok(PathKind::Directory { is_symlink: false });
        }
        if symlink {
            self.base.symlink_path_kind(path)
        } else {
            self.base.path_kind(path)
        }
        .map_err(|error| error.to_string())
    }
    fn kind(&self, path: &Utf8Path, symlink: bool) -> Result<PathKind, FileSystemDiagnostic> {
        self.kinds
            .lock()
            .entry((path.to_path_buf(), symlink))
            .or_insert_with(|| self.live_kind(path, symlink))
            .clone()
            .map_err(|_| Self::error(path))
    }
    pub fn unchanged(&self) -> bool {
        self.reads.lock().iter().all(|(path, text)| {
            self.overlays.get(path).cloned().map_or_else(
                || {
                    self.base
                        .read_file_from_path(path)
                        .map_err(|error| error.to_string())
                },
                Ok,
            ) == *text
        }) && self
            .kinds
            .lock()
            .iter()
            .all(|((path, symlink), kind)| self.live_kind(path, *symlink) == *kind)
            && self.identities.lock().iter().all(|(path, identity)| {
                self.base
                    .canonicalize_path(path)
                    .map_err(|error| (error.kind(), error.to_string()))
                    == *identity
            })
            && self.links.lock().iter().all(|(path, target)| {
                self.base
                    .read_link(path)
                    .map_err(|error| (error.kind(), error.to_string()))
                    == *target
            })
    }
}
impl FileSystem for PinnedFs {
    fn open_with_options(
        &self,
        path: &Utf8Path,
        _options: OpenOptions,
    ) -> io::Result<Box<dyn File>> {
        self.read_file_from_path(path)
            .map(|text| Box::new(PinnedFile(text)) as Box<dyn File>)
            .map_err(|error| io::Error::other(error.to_string()))
    }
    fn read_file_from_path(&self, path: &Utf8Path) -> Result<String, FileSystemDiagnostic> {
        self.reads
            .lock()
            .entry(path.to_path_buf())
            .or_insert_with(|| {
                self.overlays.get(path).cloned().map_or_else(
                    || {
                        self.base
                            .read_file_from_path(path)
                            .map_err(|error| error.to_string())
                    },
                    Ok,
                )
            })
            .clone()
            .map_err(|_| Self::error(path))
    }
    fn traversal<'scope>(
        &'scope self,
        func: Box<dyn FnOnce(&dyn TraversalScope<'scope>) + Send + '_>,
    ) {
        self.base.traversal(func);
    }
    fn working_directory(&self) -> Option<Utf8PathBuf> {
        self.base.working_directory()
    }
    fn path_exists(&self, path: &Utf8Path) -> bool {
        self.path_kind(path).is_ok()
    }
    fn path_kind(&self, path: &Utf8Path) -> Result<PathKind, FileSystemDiagnostic> {
        self.kind(path, false)
    }
    fn symlink_path_kind(&self, path: &Utf8Path) -> Result<PathKind, FileSystemDiagnostic> {
        self.kind(path, true)
    }
    fn read_link(&self, path: &Utf8Path) -> io::Result<Utf8PathBuf> {
        self.links
            .lock()
            .entry(path.into())
            .or_insert_with(|| {
                self.base
                    .read_link(path)
                    .map_err(|error| (error.kind(), error.to_string()))
            })
            .clone()
            .map_err(|(kind, message)| io::Error::new(kind, message))
    }
    fn canonicalize_path(&self, path: &Utf8Path) -> io::Result<Utf8PathBuf> {
        self.identities
            .lock()
            .entry(path.into())
            .or_insert_with(|| {
                self.base
                    .canonicalize_path(path)
                    .map_err(|error| (error.kind(), error.to_string()))
            })
            .clone()
            .map_err(|(kind, message)| io::Error::new(kind, message))
    }
    fn get_changed_files(&self, base: &str) -> io::Result<Vec<String>> {
        self.base.get_changed_files(base)
    }
    fn get_staged_files(&self) -> io::Result<Vec<String>> {
        self.base.get_staged_files()
    }
}
struct PinnedFile(String);
impl File for PinnedFile {
    fn read_to_string(&mut self, buffer: &mut String) -> io::Result<()> {
        buffer.push_str(&self.0);
        Ok(())
    }
    fn set_content(&mut self, _content: &[u8]) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "analysis inputs are immutable",
        ))
    }
    fn file_version(&self) -> i32 {
        0
    }
}
