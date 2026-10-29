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

#[test]
fn canonical_path_memory_preserves_root_and_leading_parent_identity() {
    let fs = MemoryFileSystem::default();
    for path in ["/target.ts", "target.ts", "../target.ts"] {
        fs.insert(path.into(), path);
    }
    assert_eq!(
        fs.canonicalize_path(Utf8Path::new("/../target.ts"))
            .unwrap(),
        Utf8Path::new("/target.ts")
    );
    assert_eq!(
        fs.canonicalize_path(Utf8Path::new("./../target.ts"))
            .unwrap(),
        Utf8Path::new("../target.ts")
    );
    assert_eq!(
        fs.canonicalize_path(Utf8Path::new("/folder/../../target.ts"))
            .unwrap(),
        Utf8Path::new("/target.ts")
    );
}

#[test]
fn canonical_path_memory_recognizes_stored_aliases_and_their_directories() {
    for stored in ["./project/file.ts", "project/sub/../file.ts"] {
        let fs = MemoryFileSystem::default();
        fs.insert(stored.into(), "source");
        for alias in [stored, "project/file.ts", "./project/file.ts"] {
            assert_eq!(
                fs.canonicalize_path(Utf8Path::new(alias)).unwrap(),
                Utf8Path::new("project/file.ts")
            );
        }
        for directory in ["project", "./project", "project/sub/.."] {
            assert_eq!(
                fs.canonicalize_path(Utf8Path::new(directory)).unwrap(),
                Utf8Path::new("project")
            );
        }
        assert!(!fs.path_exists(Utf8Path::new("project/file.ts")));
        assert!(fs.path_exists(Utf8Path::new(stored)));
    }
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
