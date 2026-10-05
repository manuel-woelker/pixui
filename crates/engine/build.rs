//! Embed the tool-tool managed font without requiring a developer environment.

use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=PIXUI_GEIST_DIRECTORY");
    let manifest =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets manifest directory"));
    // tool-tool's default download uses the same archive on all platforms, but
    // caches it under the host OS. Build scripts execute on the host even when
    // cross-compiling, so this must not use the package's target OS.
    let directory = env::var_os("PIXUI_GEIST_DIRECTORY").map_or_else(
        || {
            manifest.join(format!(
                "../../.cache/tool-tool/geist-1.7.0-{}",
                env::consts::OS
            ))
        },
        PathBuf::from,
    );
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo sets output directory"));
    for (relative, filename) in [
        ("fonts/Geist/ttf/Geist-Regular.ttf", "Geist-Regular.ttf"),
        (
            "fonts/GeistMono/ttf/GeistMono-Regular.ttf",
            "GeistMono-Regular.ttf",
        ),
    ] {
        let source = directory.join(relative);
        println!("cargo:rerun-if-changed={}", source.display());
        fs::copy(&source, output.join(filename)).unwrap_or_else(|error| {
            panic!("Cannot embed {filename} from {}: {error}. Download the pinned font through tool-tool (run ./t cargo check from the repository root), then retry. PIXUI_GEIST_DIRECTORY may optionally override its cache directory.", source.display())
        });
    }
}
