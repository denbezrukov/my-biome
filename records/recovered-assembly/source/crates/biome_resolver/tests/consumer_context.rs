use biome_fs::MemoryFileSystem;
use biome_resolver::{DiscoverableManifest, ResolveOptions, ResolverFsProxy, resolve};
use camino::{Utf8Path, Utf8PathBuf};

fn fixture() -> MemoryFileSystem {
    let fs = MemoryFileSystem::default();
    for (path, text) in [
        (
            "/app/package.json",
            r##"{"name":"self","imports":{"#local":"./mapped.js"},"exports":{"./api":"./mapped.js"}}"##,
        ),
        (
            "/app/tsconfig.json",
            r#"{"compilerOptions":{"paths":{"mapped":["./mapped.js"]},"typeRoots":["./types"]}}"#,
        ),
        ("/app/mapped.js", ""),
        ("/app/src/mapped.js", ""),
        (
            "/app/src/node_modules/pkg/package.json",
            r#"{"name":"inner","main":"index.js"}"#,
        ),
        ("/app/src/node_modules/pkg/index.js", ""),
        (
            "/app/node_modules/pkg/package.json",
            r#"{"name":"outer","main":"index.js"}"#,
        ),
        ("/app/node_modules/pkg/index.js", ""),
        (
            "/app/node_modules/fallback/package.json",
            r#"{"name":"fallback","main":"index.js"}"#,
        ),
        ("/app/node_modules/fallback/index.js", ""),
        ("/app/types/typed/package.json", r#"{"types":"index.d.ts"}"#),
        ("/app/types/typed/index.d.ts", ""),
        (
            "/app/src/types/typed/package.json",
            r#"{"types":"index.d.ts"}"#,
        ),
        ("/app/src/types/typed/index.d.ts", ""),
    ] {
        fs.insert(path.into(), text);
    }
    fs
}

fn nested_and_fallback(explicit: Option<bool>) {
    let fs = fixture();
    let package = fs
        .read_package_json_in_directory(Utf8Path::new("/app"))
        .unwrap();
    let options = ResolveOptions {
        package_json: match explicit {
            Some(true) => DiscoverableManifest::Explicit {
                package_path: "/app".into(),
                manifest: &package,
            },
            Some(false) => DiscoverableManifest::Auto,
            None => DiscoverableManifest::Off,
        },
        extensions: &["js"],
        default_files: &["index"],
        ..Default::default()
    };
    for (request, expected) in [
        ("pkg", "/app/src/node_modules/pkg/index.js"),
        ("fallback", "/app/node_modules/fallback/index.js"),
    ] {
        assert_eq!(
            resolve(request, Utf8Path::new("/app/src"), &fs, &options),
            Ok(Utf8PathBuf::from(expected))
        );
    }
}

#[test]
fn automatic_manifest_keeps_dependency_consumer_directory() {
    nested_and_fallback(Some(false));
}

#[test]
fn explicit_manifest_keeps_dependency_consumer_directory() {
    nested_and_fallback(Some(true));
}

#[test]
fn disabled_manifest_keeps_dependency_consumer_directory() {
    nested_and_fallback(None);
}

#[test]
fn package_mappings_and_type_roots_keep_manifest_anchor() {
    let fs = fixture();
    let package = fs
        .read_package_json_in_directory(Utf8Path::new("/app"))
        .unwrap();
    for manifest in [
        DiscoverableManifest::Auto,
        DiscoverableManifest::Explicit {
            package_path: "/app".into(),
            manifest: &package,
        },
    ] {
        let options = ResolveOptions {
            package_json: manifest,
            extensions: &["js", "ts"],
            default_files: &["index"],
            ..Default::default()
        };
        for request in ["#local", "self/api", "mapped"] {
            assert_eq!(
                resolve(request, Utf8Path::new("/app/src"), &fs, &options),
                Ok(Utf8PathBuf::from("/app/mapped.js"))
            );
        }
        assert_eq!(
            resolve(
                "typed",
                Utf8Path::new("/app/src"),
                &fs,
                &ResolveOptions {
                    resolve_types: true,
                    ..options
                }
            ),
            Ok(Utf8PathBuf::from("/app/types/typed/index.d.ts"))
        );
    }
}
