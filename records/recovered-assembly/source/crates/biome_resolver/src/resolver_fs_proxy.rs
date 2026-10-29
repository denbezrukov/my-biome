use biome_diagnostics::{Diagnostic, Error, PrintDescription, Severity};
use biome_fs::{FileSystem, FsErrorKind, PathKind, expand_symbolic_link};
use biome_package::{Manifest, PackageJson, TsConfigJson};
use biome_rowan::TextRange;
use camino::{Utf8Path, Utf8PathBuf};

use crate::errors::ResolveError;

/// Represents the kind of filesystem entry a path points at.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PathInfo {
    Directory,
    File,
    Symlink {
        /// The canonicalized target of the symlink.
        ///
        /// Accessing this target is guaranteed to not return another symlink.
        canonicalized_target: Utf8PathBuf,
    },
}

impl PathInfo {
    pub const fn is_symlink(&self) -> bool {
        matches!(self, Self::Symlink { .. })
    }
}

/// A proxy for handling requests to the filesystem.
///
/// The proxy only implements a very specific subset of FS functionality in such
/// a way that we can utilise our module graph as a FS proxy instead of always
/// performing ad-hoc FS I/O.
pub trait ResolverFsProxy {
    /// Returns quality from the same read that produces the manifest.
    /// Proxies implementing only legacy reads cannot establish parse validity.
    fn find_package_json_with_quality(
        &self,
        search_dir: &Utf8Path,
    ) -> Result<(Utf8PathBuf, ManifestRead<PackageJson>), ResolveError> {
        self.find_package_json(search_dir).map(|(path, manifest)| {
            (
                path,
                ManifestRead {
                    manifest: Some(manifest),
                    quality: ManifestQuality::default(),
                },
            )
        })
    }

    fn read_package_json_with_quality(
        &self,
        dir_path: &Utf8Path,
    ) -> Result<ManifestRead<PackageJson>, ResolveError> {
        self.read_package_json_in_directory(dir_path)
            .map(|manifest| ManifestRead {
                manifest: Some(manifest),
                quality: ManifestQuality::default(),
            })
    }

    fn read_tsconfig_json_with_quality(
        &self,
        path: &Utf8Path,
    ) -> Result<ManifestRead<TsConfigJson>, ResolveError> {
        self.read_tsconfig_json(path).map(|manifest| ManifestRead {
            manifest: Some(manifest),
            quality: ManifestQuality::default(),
        })
    }

    /// Finds the `package.json` in `search_dir` or one of its parents.
    ///
    /// Returns both the parsed `PackageJson` structure as well as the path of
    /// the package in which it was found.
    fn find_package_json(
        &self,
        search_dir: &Utf8Path,
    ) -> Result<(Utf8PathBuf, PackageJson), ResolveError>;

    /// Returns information about the path.
    ///
    /// This method does not follow symlinks.
    ///
    /// Errors if the path doesn't exist , isn't accessible, or if the path
    /// points to something that is not a file, directory, or symlink.
    fn path_info(&self, path: &Utf8Path) -> Result<PathInfo, ResolveError>;

    /// Reads the `package.json` manifest that is expected to exist in the
    /// directory with the given path.
    fn read_package_json_in_directory(
        &self,
        dir_path: &Utf8Path,
    ) -> Result<PackageJson, ResolveError>;

    /// Reads the root `tsconfig.json` manifest in the given directory.
    fn read_tsconfig_json_in_directory(
        &self,
        dir_path: &Utf8Path,
    ) -> Result<TsConfigJson, ResolveError> {
        self.read_tsconfig_json(&dir_path.join("tsconfig.json"))
    }

    /// Reads a TypeScript configuration manifest at the exact given path.
    fn read_tsconfig_json(&self, path: &Utf8Path) -> Result<TsConfigJson, ResolveError>;
}

pub trait FsWithResolverProxy: FileSystem + ResolverFsProxy {}

impl<Fs: FileSystem> FsWithResolverProxy for Fs {}

impl<Fs: FileSystem> ResolverFsProxy for Fs {
    fn find_package_json_with_quality(
        &self,
        search_dir: &Utf8Path,
    ) -> Result<(Utf8PathBuf, ManifestRead<PackageJson>), ResolveError> {
        let found = self
            .auto_search_files(search_dir, &["package.json"])
            .ok_or(ResolveError::NotFound)?;
        let read = self.read_package_json_with_quality(&found.directory_path)?;
        Ok((found.directory_path, read))
    }

    fn read_package_json_with_quality(
        &self,
        dir_path: &Utf8Path,
    ) -> Result<ManifestRead<PackageJson>, ResolveError> {
        let (manifest, diagnostics) =
            PackageJson::read_manifest(self, &dir_path.join("package.json")).consume();
        if manifest.is_none() && self.path_info(&dir_path.join("package.json")).is_err() {
            return Err(ResolveError::ErrorLoadingManifest);
        }
        Ok(ManifestRead::from_parts(manifest, &diagnostics))
    }

    fn read_tsconfig_json_with_quality(
        &self,
        path: &Utf8Path,
    ) -> Result<ManifestRead<TsConfigJson>, ResolveError> {
        let (manifest, diagnostics) = TsConfigJson::read_manifest(self, path).consume();
        if manifest.is_none() && self.path_info(path).is_err() {
            return Err(ResolveError::ErrorLoadingManifest);
        }
        Ok(ManifestRead::from_parts(manifest, &diagnostics))
    }

    fn find_package_json(
        &self,
        search_dir: &Utf8Path,
    ) -> Result<(Utf8PathBuf, PackageJson), ResolveError> {
        self.auto_search_files(search_dir, &["package.json"])
            .ok_or(ResolveError::NotFound)
            .and_then(|result| {
                self.read_package_json_in_directory(&result.directory_path)
                    .map(|manifest| (result.directory_path, manifest))
            })
    }

    fn path_info(&self, path: &Utf8Path) -> Result<PathInfo, ResolveError> {
        match self.symlink_path_kind(path) {
            Ok(PathKind::Directory { .. }) => Ok(PathInfo::Directory),
            Ok(PathKind::File { is_symlink }) if is_symlink => match expand_symbolic_link(path) {
                Ok((normalized_target, _)) => Ok(PathInfo::Symlink {
                    canonicalized_target: normalized_target,
                }),
                Err(_error) => Err(ResolveError::BrokenSymlink),
            },
            Ok(PathKind::File { .. }) => Ok(PathInfo::File),
            Err(error) => match error.error_kind {
                FsErrorKind::DereferencedSymlink | FsErrorKind::DeeplyNestedSymlinkExpansion => {
                    Err(ResolveError::BrokenSymlink)
                }
                _ => Err(ResolveError::NotFound),
            },
        }
    }

    fn read_package_json_in_directory(
        &self,
        dir_path: &Utf8Path,
    ) -> Result<PackageJson, ResolveError> {
        match PackageJson::read_manifest(self, &dir_path.join("package.json")).consume() {
            (Some(manifest), _errors) => Ok(manifest),
            _ => Err(ResolveError::ErrorLoadingManifest),
        }
    }

    fn read_tsconfig_json(&self, path: &Utf8Path) -> Result<TsConfigJson, ResolveError> {
        match TsConfigJson::read_manifest(self, path).consume() {
            (Some(manifest), _errors) => Ok(manifest),
            _ => Err(ResolveError::ErrorLoadingManifest),
        }
    }
}

/// Owned diagnostics attached to the manifest value from the same parse.
#[derive(Clone, Debug)]
pub struct ManifestRead<M> {
    pub manifest: Option<M>,
    pub quality: ManifestQuality,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ManifestQuality {
    pub known: bool,
    pub diagnostics: Vec<ManifestDiagnostic>,
}

impl ManifestQuality {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity >= Severity::Error)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManifestDiagnostic {
    pub severity: Severity,
    pub message: String,
    pub range: Option<TextRange>,
}

impl ManifestDiagnostic {
    pub(crate) fn from_diagnostic(diagnostic: &impl Diagnostic) -> Self {
        Self {
            severity: diagnostic.severity(),
            message: PrintDescription(diagnostic).to_string(),
            range: diagnostic.location().span,
        }
    }
}

pub(crate) trait ResolverManifest: Manifest {
    fn metadata_diagnostics(&self) -> &[biome_diagnostics::serde::Diagnostic];
}

impl ResolverManifest for PackageJson {
    fn metadata_diagnostics(&self) -> &[biome_diagnostics::serde::Diagnostic] {
        &self.project_metadata.diagnostics
    }
}

impl ResolverManifest for TsConfigJson {
    fn metadata_diagnostics(&self) -> &[biome_diagnostics::serde::Diagnostic] {
        &self.metadata_diagnostics
    }
}

impl<M> ManifestRead<M> {
    pub(crate) fn from_parts(manifest: Option<M>, diagnostics: &[Error]) -> Self
    where
        M: ResolverManifest,
    {
        let mut quality = ManifestQuality {
            known: true,
            diagnostics: diagnostics
                .iter()
                .map(|diagnostic| ManifestDiagnostic {
                    severity: diagnostic.severity(),
                    message: PrintDescription(diagnostic).to_string(),
                    range: diagnostic.location().span,
                })
                .collect(),
        };
        if let Some(manifest) = &manifest {
            quality.diagnostics.extend(
                manifest
                    .metadata_diagnostics()
                    .iter()
                    .map(ManifestDiagnostic::from_diagnostic),
            );
        }
        Self { manifest, quality }
    }
}
