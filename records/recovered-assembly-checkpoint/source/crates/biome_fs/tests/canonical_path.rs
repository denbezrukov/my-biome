use biome_fs::{FileSystem, MemoryFileSystem, TemporaryFs};
use camino::Utf8Path;
use std::{io::ErrorKind, sync::Arc};

#[test]
fn canonical_path_memory_preserves_case_and_missing_errors() {
    let fs = Arc::new(MemoryFileSystem::default());
    fs.insert("/project/File.ts".into(), "upper");
    fs.insert("/project/file.ts".into(), "lower");
    assert_eq!(
        fs.canonicalize_path(Utf8Path::new("/project/./File.ts"))
            .unwrap(),
        Utf8Path::new("/project/File.ts")
    );
    assert_ne!(
        fs.canonicalize_path(Utf8Path::new("/project/File.ts"))
            .unwrap(),
        fs.canonicalize_path(Utf8Path::new("/project/file.ts"))
            .unwrap()
    );
    assert_eq!(
        fs.canonicalize_path(Utf8Path::new("/project/missing.ts"))
            .unwrap_err()
            .kind(),
        ErrorKind::NotFound
    );
}

#[cfg(unix)]
#[test]
fn canonical_path_os_resolves_directory_aliases_and_rejects_cycles() {
    let mut fixture = TemporaryFs::new("biome-fs-a3-canonical-path");
    let source = fixture.create_file("src/File.ts", "source");
    std::os::unix::fs::symlink(
        source.parent().unwrap(),
        fixture.working_directory.join("alias"),
    )
    .unwrap();
    std::os::unix::fs::symlink("cycle-b", fixture.working_directory.join("cycle-a")).unwrap();
    std::os::unix::fs::symlink("cycle-a", fixture.working_directory.join("cycle-b")).unwrap();
    let fs = Arc::new(fixture.create_os());
    assert_eq!(
        fs.canonicalize_path(&fixture.working_directory.join("alias/File.ts"))
            .unwrap(),
        source
    );
    assert!(
        fs.canonicalize_path(&fixture.working_directory.join("cycle-a"))
            .is_err()
    );
    assert_eq!(
        fs.canonicalize_path(&fixture.working_directory.join("missing"))
            .unwrap_err()
            .kind(),
        ErrorKind::NotFound
    );
    #[cfg(target_os = "linux")]
    {
        let lower = fixture.create_file("src/file.ts", "distinct");
        assert_ne!(
            fs.canonicalize_path(&source).unwrap(),
            fs.canonicalize_path(&lower).unwrap()
        );
    }
}

#[cfg(windows)]
#[test]
fn canonical_path_windows_uses_filesystem_spelling() {
    let mut fixture = TemporaryFs::new("biome-fs-a3-canonical-windows");
    let source = fixture.create_file("MixedCase.ts", "source");
    let fs = Arc::new(fixture.create_os());
    let canonical = fs.canonicalize_path(&source).unwrap();
    assert_eq!(
        fs.canonicalize_path(&fixture.working_directory.join("mixedcase.ts"))
            .unwrap(),
        canonical
    );
    assert_eq!(canonical.file_name(), Some("MixedCase.ts"));
}
