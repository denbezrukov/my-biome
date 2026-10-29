use biome_configuration::project_analysis::ProjectPattern;
use biome_fs::{FileSystem, normalize_path};
use biome_glob::NormalizedGlob;
use biome_project_analysis::options::AnalysisMode;
use camino::{Utf8Path, Utf8PathBuf};

pub(super) fn join(base: &Utf8Path, path: &str) -> Utf8PathBuf {
    normalize_path(&base.join(path.replace('\\', "/")))
}

pub(super) fn canonical(fs: &dyn FileSystem, path: &Utf8Path) -> Result<Utf8PathBuf, String> {
    let path = normalize_path(path);
    for ancestor in path.ancestors() {
        match fs.canonicalize_path(ancestor) {
            Ok(canonical) => {
                return Ok(normalize_path(
                    &canonical.join(path.strip_prefix(ancestor).unwrap()),
                ));
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && !fs.path_is_symlink(ancestor) => {}
            Err(error) => return Err(format!("Cannot identify {path}: {error}")),
        }
    }
    Err(format!("No existing ancestor for {path}"))
}

// The URL crate's OS-path constructor is unavailable on wasm32-unknown-unknown.
#[cfg(not(windows))]
pub(super) fn portable_file_uri(path: &Utf8Path) -> String {
    // wasm32's OS-independent Path does not classify rooted virtual paths as absolute.
    #[cfg(target_arch = "wasm32")]
    if !path.has_root() {
        return path.to_string();
    }
    #[cfg(not(target_arch = "wasm32"))]
    if !path.is_absolute() {
        return path.to_string();
    }
    // Match the URL file-path encoder, including literal controls that URL setters discard.
    const SEGMENT: &percent_encoding::AsciiSet = &percent_encoding::CONTROLS
        .add(b' ')
        .add(b'"')
        .add(b'<')
        .add(b'>')
        .add(b'`')
        .add(b'#')
        .add(b'?')
        .add(b'{')
        .add(b'}')
        .add(b'/')
        .add(b'%')
        .add(b'\\');
    let mut uri = String::from("file://");
    for component in path.iter().skip(1) {
        uri.push('/');
        uri.extend(percent_encoding::utf8_percent_encode(component, SEGMENT));
    }
    if uri == "file://" {
        uri.push('/');
    }
    uri
}

fn glob_path(value: &str) -> String {
    let value = value.replace('\\', "/");
    if let Some(value) = value.strip_prefix("//?/UNC/") {
        format!("//{value}")
    } else {
        value.strip_prefix("//?/").unwrap_or(&value).to_string()
    }
}

pub(super) fn is_source(path: &Utf8Path) -> bool {
    matches!(
        path.extension(),
        Some("js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts")
    )
}

pub(super) fn is_declaration(path: &Utf8Path) -> bool {
    [".d.ts", ".d.mts", ".d.cts"]
        .iter()
        .any(|suffix| path.as_str().ends_with(suffix))
}

pub(super) struct Patterns(Vec<(NormalizedGlob, bool)>);
impl Patterns {
    pub(super) fn new(values: &[String], mode: AnalysisMode, marked: bool) -> (Self, Vec<String>) {
        let mut patterns = Vec::new();
        let mut errors = Vec::new();
        for value in values {
            let parsed = ProjectPattern::new(value);
            if marked && mode == AnalysisMode::Default && parsed.negated && parsed.production {
                continue;
            }
            let negative =
                parsed.negated || (marked && mode != AnalysisMode::Default && !parsed.production);
            let pattern = if marked {
                parsed.pattern
            } else {
                value.trim_start_matches('!').to_string()
            };
            match glob_path(&pattern).parse::<NormalizedGlob>() {
                Ok(glob) => patterns.push((glob, negative)),
                Err(error) => errors.push(format!("Unsupported pattern {value}: {error}")),
            }
        }
        if marked {
            // Entry/project discovery applies exclusions last; workspace policies retain sequence.
            patterns.sort_by_key(|(_, negative)| *negative);
        }
        (Self(patterns), errors)
    }
    pub(super) fn matches(&self, path: &Utf8Path) -> bool {
        self.0
            .iter()
            .rev()
            .find(|(pattern, _)| pattern.is_raw_match(glob_path(path.as_str())))
            .is_some_and(|(_, negative)| !negative)
    }
}

pub(super) fn expand_export(pattern: &str) -> String {
    if let Some(prefix) = pattern.strip_suffix("/*") {
        return format!("{prefix}/**");
    }
    pattern
        .replacen("/*.", "/**/*.", 1)
        .replacen("/*/", "/**/", 1)
}

pub(super) fn source_pattern(pattern: &Utf8Path) -> String {
    let value = pattern.as_str();
    for extension in [".d.ts", ".d.mts", ".d.cts", ".js", ".jsx", ".mjs", ".cjs"] {
        if let Some(stem) = value.strip_suffix(extension) {
            return format!("{stem}.{{ts,tsx,mts,cts,js,jsx,mjs,cjs}}");
        }
    }
    value.to_string()
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    #[test]
    fn file_uris_preserve_source_identity_and_escape_segments() {
        for (path, expected) in [
            (
                "/project/café #%.ts",
                "file:///project/caf%C3%A9%20%23%25.ts",
            ),
            ("/project/a?b.ts", "file:///project/a%3Fb.ts"),
            ("/project/a\\b.ts", "file:///project/a%5Cb.ts"),
            ("/project/a\tb\nc\r.ts", "file:///project/a%09b%0Ac%0D.ts"),
            ("/project/../file.ts", "file:///project/../file.ts"),
            ("/", "file:///"),
            ("relative/index.ts", "relative/index.ts"),
        ] {
            assert_eq!(portable_file_uri(Utf8Path::new(path)), expected);
            #[cfg(unix)]
            assert_eq!(
                portable_file_uri(Utf8Path::new(path)),
                url::Url::from_file_path(path)
                    .map_or_else(|_| path.to_string(), |url| url.to_string())
            );
        }
    }
}
