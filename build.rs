//! Builds the Shortcuts actions (macos/intents/Intents.swift, App Intents)
//! into a static library linked into the app binary, and generates the
//! `Metadata.appintents` that Shortcuts reads, next to the binary in
//! `target/<profile>/` (scripts/bundle.sh copies it into the app).
//!
//! Needs Xcode (for the Swift compiler and the App Intents metadata tool).
//! Set `PIXLSNAPZ_NO_INTENTS=1` to build without the Shortcuts actions.

use std::path::{Path, PathBuf};
use std::process::Command;

const SOURCE: &str = "macos/intents/Intents.swift";
const MODULE: &str = "PixlSnapzIntents";
/// Matches `MACOSX_DEPLOYMENT_TARGET` in .cargo/config.toml.
const DEPLOYMENT_TARGET: &str = "14.0";
/// The protocols whose conformances the Swift compiler records as constant
/// values for the metadata tool (as Xcode passes them).
const PROTOCOLS: &str = r#"["AppIntent","EntityQuery","AppEntity","TransientEntity","AppEnum","AppShortcutProviding","AppShortcutsProvider","AnyResolverProviding","AppIntentsPackage","DynamicOptionsProvider"]"#;

fn main() {
    println!("cargo:rerun-if-changed={SOURCE}");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=PIXLSNAPZ_NO_INTENTS");
    if std::env::var_os("PIXLSNAPZ_NO_INTENTS").is_some()
        || std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos")
    {
        return;
    }
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("set by Cargo"));
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("set by Cargo"));
    let source = manifest.join(SOURCE);

    let sdk = xcrun(&["--show-sdk-path"]);
    let swiftc = xcrun(&["--find", "swiftc"]);
    // .../XcodeDefault.xctoolchain/usr/bin/swiftc -> the toolchain.
    let toolchain = Path::new(&swiftc).ancestors().nth(3).expect("swiftc is in a toolchain").to_path_buf();
    let triple = format!("arm64-apple-macos{DEPLOYMENT_TARGET}");

    let protocols = out.join("protocols.json");
    std::fs::write(&protocols, PROTOCOLS).expect("writing protocols.json");
    let library = out.join(format!("lib{MODULE}.a"));
    let constants = out.join("Intents.swiftconstvalues");
    run(Command::new(&swiftc)
        .args(["-parse-as-library", "-wmo", "-O", "-module-name", MODULE, "-target", &triple, "-sdk", &sdk])
        .args(["-emit-library", "-static", "-o"])
        .arg(&library)
        .arg("-emit-const-values-path")
        .arg(&constants)
        .args(["-Xfrontend", "-const-gather-protocols-file", "-Xfrontend"])
        .arg(&protocols)
        .arg(&source));

    // The metadata: written to OUT_DIR, then moved next to the binary.
    let lists = |name: &str, lines: &[&Path]| {
        let path = out.join(name);
        let text: Vec<String> = lines.iter().map(|p| p.display().to_string()).collect();
        std::fs::write(&path, text.join("\n")).expect("writing a file list");
        path
    };
    let sources = lists("sources.txt", &[&source]);
    let constant_list = lists("constvals.txt", &[&constants]);
    let empty = lists("empty.txt", &[]);
    let metadata_out = out.join("metadata");
    let _ = std::fs::remove_dir_all(&metadata_out);
    std::fs::create_dir_all(&metadata_out).expect("creating the metadata folder");
    let xcode_version = Command::new("xcodebuild")
        .arg("-version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|t| t.lines().find_map(|l| l.strip_prefix("Build version ").map(str::to_string)))
        .unwrap_or_else(|| "unknown".to_string());
    run(Command::new("xcrun")
        .arg("appintentsmetadataprocessor")
        .arg("--output")
        .arg(&metadata_out)
        .arg("--toolchain-dir")
        .arg(&toolchain)
        .args(["--module-name", MODULE, "--sdk-root", &sdk, "--xcode-version", &xcode_version])
        .args(["--platform-family", "macOS", "--deployment-target", DEPLOYMENT_TARGET, "--target-triple", &triple])
        .arg("--source-file-list")
        .arg(&sources)
        .arg("--swift-const-vals-list")
        .arg(&constant_list)
        .arg("--metadata-file-list")
        .arg(&empty)
        .arg("--static-metadata-file-list")
        .arg(&empty)
        .args(["--force", "--deployment-aware-processing", "--no-app-shortcuts-localization"]));
    // OUT_DIR is target/<profile>/build/<crate>-<hash>/out.
    if let Some(profile_dir) = out.ancestors().nth(3) {
        let target = profile_dir.join("Metadata.appintents");
        let _ = std::fs::remove_dir_all(&target);
        copy_dir(&metadata_out.join("Metadata.appintents"), &target);
    }

    // Linked into the app only: tests do not need the Shortcuts actions.
    // Nothing in Rust calls the Swift code, so the whole library is loaded.
    println!("cargo:rustc-link-arg-bins=-Wl,-force_load,{}", library.display());
    println!("cargo:rustc-link-arg-bins=-L{}/usr/lib/swift", sdk);
    println!("cargo:rustc-link-arg-bins=-L{}/usr/lib/swift/macosx", toolchain.display());
    println!("cargo:rustc-link-arg-bins=-Wl,-rpath,/usr/lib/swift");
    println!("cargo:rustc-link-arg-bins=-framework");
    println!("cargo:rustc-link-arg-bins=AppIntents");
}

fn xcrun(args: &[&str]) -> String {
    let output = Command::new("xcrun").args(args).output().expect("xcrun (Xcode) is needed for the Shortcuts actions");
    assert!(output.status.success(), "xcrun {args:?} failed: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).expect("UTF-8 output").trim().to_string()
}

fn run(command: &mut Command) {
    let output = command.output().unwrap_or_else(|e| panic!("running {command:?}: {e}"));
    if !output.status.success() {
        panic!(
            "{command:?} failed:\n{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("creating a folder");
    for entry in std::fs::read_dir(from).expect("reading the metadata").flatten() {
        let path = entry.path();
        let dest = to.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &dest);
        } else {
            std::fs::copy(&path, &dest).expect("copying the metadata");
        }
    }
}
