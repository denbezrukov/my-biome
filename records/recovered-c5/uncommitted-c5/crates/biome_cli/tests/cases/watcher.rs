use crate::TestArgs as Args;
use crate::run_cli_with_watcher_factory;
use crate::snap_test::markup_to_string;
use biome_cli::{MockWatcher, WatcherEvent};
use biome_console::{BufferConsole, markup};
use biome_fs::TemporaryFs;
use std::sync::Mutex;

#[test]
fn changed_output_does_not_shrink_watch_roots() {
    use crate::snap_test::{SnapshotPayload, assert_cli_snapshot};
    use biome_fs::MemoryFileSystem;
    use std::sync::Arc;
    struct Roots(Arc<Mutex<Vec<camino::Utf8PathBuf>>>);
    impl biome_cli::Watcher for Roots {
        fn watch(&mut self, paths: Vec<camino::Utf8PathBuf>) { self.0.lock().unwrap().extend(paths); }
        fn poll(&mut self) -> Option<WatcherEvent> { None }
    }
    let fs = MemoryFileSystem::default();
    fs.insert("/project/biome.json".into(), r#"{"projectAnalysis":{"enabled":true}}"#);
    fs.insert("/project/package.json".into(), r#"{"private":true}"#);
    fs.insert("/project/index.ts".into(), "import '../external/consumer'; export { value } from './lib';");
    fs.insert("/project/lib.ts".into(), "export const value = 1;");
    fs.insert("/external/consumer.ts".into(), "import { value } from '../project/lib'; console.log(value);");
    let roots = Arc::new(Mutex::new(Vec::new()));
    let capture = roots.clone();
    let mut console = BufferConsole::default();
    let result = run_cli_with_watcher_factory(
        Box::new(MemoryFileSystem::from_files(fs.files.0.clone())), &mut console,
        Args::from(["analyze", "--watch", "--config-path=/project", "/project/lib.ts"].as_slice()),
        Box::new(move || Box::new(Roots(capture.clone()))),
    );
    let watched = roots.lock().unwrap();
    assert!(watched.iter().any(|path| path == "/project"), "{watched:?}");
    assert!(watched.iter().any(|path| path == "/external"), "{watched:?}");
    assert_cli_snapshot(SnapshotPayload::new(module_path!(), "changed_output_does_not_shrink_watch_roots", fs, console, result));
}

#[test]
fn watch_local_write_refreshes_shared_storage() {
    use crate::snap_test::{SnapshotPayload, assert_cli_snapshot};
    use biome_fs::MemoryFileSystem;

    let fs = MemoryFileSystem::default();
    fs.insert("biome.json".into(), r#"{"projectAnalysis":{"enabled":true},"linter":{"enabled":false}}"#);
    fs.insert("package.json".into(), r#"{"private":true,"exports":"./index.ts"}"#);
    fs.insert("index.ts".into(), "export { value } from './value';\n");
    fs.insert("value.ts".into(), "export const value = 1;\n");
    let files = fs.files.clone();
    let mut console = BufferConsole::default();
    let result = run_cli_with_watcher_factory(
        Box::new(MemoryFileSystem::from_files(files.0.clone())),
        &mut console,
        Args::from(["lint", "--watch", "."].as_slice()),
        Box::new(move || {
            let fs = MemoryFileSystem::from_files(files.0.clone());
            fs.insert("index.ts".into(), "export const replacement = 1;\n");
            Box::new(MockWatcher::with_events(vec![WatcherEvent::Changed(vec!["index.ts".into()])]))
        }),
    );
    let out = console.out_buffer.iter().map(|m| markup_to_string(markup! {{m.content}})).collect::<Vec<_>>().join("\n");
    assert_eq!(out.matches("Project analysis coverage:").count(), 2, "{out}");
    assert!(out.contains("value.ts"), "new unused source must be reported: {out}");
    assert_cli_snapshot(SnapshotPayload::new(module_path!(), "watch_local_write_refreshes_shared_storage", fs, console, result));
}

#[test]
fn lint_watch_reruns_on_event() {
    let mut fs = TemporaryFs::new("lint_watch_reruns_on_event");
    fs.create_file(
        "biome.json",
        r#"{"linter":{"rules":{"preset":"recommended","suspicious":{"noVar":"error"}}}}"#,
    );
    fs.create_file("good.js", "const x = 1;\n");
    let bad_path = fs.create_file("bad.js", "var y = 2;\n");

    let mut console = BufferConsole::default();

    let seed = Mutex::new(Some(vec![WatcherEvent::Changed(vec![bad_path.clone()])]));
    let watcher_factory = Box::new(move || {
        let events = seed.lock().unwrap().take().unwrap_or_default();
        Box::new(MockWatcher::with_events(events)) as Box<dyn biome_cli::Watcher>
    });

    let result = run_cli_with_watcher_factory(
        Box::new(fs.create_os()),
        &mut console,
        Args::from(["lint", "--watch", fs.cli_path()].as_slice()),
        watcher_factory,
    );

    let out = console
        .out_buffer
        .iter()
        .map(|m| markup_to_string(markup! { {m.content} }))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(result.is_ok(), "run_cli returned {result:?}\n---\n{out}");

    assert!(
        out.contains("Watching for changes"),
        "expected watch banner in output, got:\n{out}"
    );

    assert_eq!(
        out.matches("noVar").count(),
        2,
        "expected lint to run twice on bad.js, got:\n{out}"
    );
}
