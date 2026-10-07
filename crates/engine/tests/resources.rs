//! Filesystem contracts independent of decoding or native windows.
use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::resources::{
    directory::DirectoryFilesystem,
    filesystem::{ResourceFilesystem, ResourceReader},
    layered::LayeredFilesystem,
    path::ResourcePath,
};
use std::{
    io::{Cursor, Read},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

#[test]
fn paths_are_relative_and_unambiguous() {
    for invalid in [
        "",
        "/a",
        "a/",
        "a//b",
        ".",
        "./a",
        "..",
        "a/../b",
        "a/./b",
        "C:/a",
        "C:a",
        "\\\\server\\a",
        "a\\b",
        "a\0b",
    ] {
        assert!(ResourcePath::new(invalid).is_err(), "{invalid:?}");
    }
    for valid in ["icons/add.png", "日本語.png", "file..png", "a b.png"] {
        assert_eq!(ResourcePath::new(valid).unwrap().as_str(), valid);
    }
}

struct Source {
    calls: Arc<AtomicUsize>,
    value: Option<&'static [u8]>,
    fail: bool,
}
impl ResourceFilesystem for Source {
    fn open(&self, _: &ResourcePath) -> PixuiResult<Option<ResourceReader>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err(pixui_error!("source failed"));
        }
        Ok(self
            .value
            .map(|bytes| Box::new(Cursor::new(bytes)) as ResourceReader))
    }
}
fn contents(source: &dyn ResourceFilesystem) -> PixuiResult<Option<Vec<u8>>> {
    source
        .open(&ResourcePath::new("a").unwrap())?
        .map(|mut reader| {
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).unwrap();
            Ok(bytes)
        })
        .transpose()
}
#[test]
fn layering_stops_at_first_file_or_error_and_can_nest() {
    let calls = Arc::new(AtomicUsize::new(0));
    let source = |value, fail| {
        Arc::new(Source {
            calls: calls.clone(),
            value,
            fail,
        }) as Arc<dyn ResourceFilesystem>
    };
    let nested = Arc::new(LayeredFilesystem::new(vec![
        source(None, false),
        source(Some(b"override"), false),
    ]));
    let layers = LayeredFilesystem::new(vec![nested, source(Some(b"default"), false)]);
    assert_eq!(contents(&layers).unwrap().unwrap(), b"override");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    calls.store(0, Ordering::SeqCst);
    assert!(
        contents(&LayeredFilesystem::new(vec![
            source(None, true),
            source(Some(b"default"), false)
        ]))
        .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(contents(&LayeredFilesystem::new(vec![])).unwrap(), None);
    assert_eq!(
        contents(&LayeredFilesystem::new(vec![
            source(Some(b""), false),
            source(Some(b"default"), false)
        ]))
        .unwrap(),
        Some(vec![])
    );
}

#[test]
fn directory_readers_are_independent_and_owned() {
    let directory =
        DirectoryFilesystem::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets")).unwrap();
    let path = ResourcePath::new("images/pixui-logo.png").unwrap();
    let mut first = directory.open(&path).unwrap().unwrap();
    let mut second = directory.open(&path).unwrap().unwrap();
    drop(directory);
    let mut signature = [0; 8];
    first.read_exact(&mut signature).unwrap();
    assert_eq!(&signature, b"\x89PNG\r\n\x1a\n");
    second.read_exact(&mut signature).unwrap();
    assert_eq!(&signature, b"\x89PNG\r\n\x1a\n");
    let directory = DirectoryFilesystem::new(env!("CARGO_MANIFEST_DIR")).unwrap();
    assert!(
        directory
            .open(&ResourcePath::new("not-present").unwrap())
            .unwrap()
            .is_none()
    );
    assert!(directory.open(&ResourcePath::new("src").unwrap()).is_err());
    assert!(DirectoryFilesystem::new("/nonexistent-pixui-resource-root").is_err());
    assert!(DirectoryFilesystem::new(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).is_err());
}

#[cfg(unix)]
#[test]
fn directory_rejects_escaping_symlinks_but_accepts_internal_ones() {
    let root = std::env::temp_dir().join(format!("pixui-resource-links-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    std::fs::write(root.join("empty"), []).unwrap();
    std::os::unix::fs::symlink("empty", root.join("internal")).unwrap();
    std::os::unix::fs::symlink(env!("CARGO_MANIFEST_DIR"), root.join("escape")).unwrap();
    let directory = DirectoryFilesystem::new(&root).unwrap();
    assert_eq!(
        contents(&DirectoryFilesystem::new(&root).unwrap()).unwrap(),
        None
    );
    assert!(
        directory
            .open(&ResourcePath::new("internal").unwrap())
            .unwrap()
            .is_some()
    );
    assert!(
        directory
            .open(&ResourcePath::new("escape/Cargo.toml").unwrap())
            .is_err()
    );
}
