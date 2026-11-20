use base64::{Engine, engine::general_purpose::STANDARD};
use flate2::{Compression, write::GzEncoder};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
};
use wasm_bindgen_cli_support::Bindgen;

fn main() {
    for path in ["web", "client", "Cargo.toml", "Cargo.lock"] {
        println!("cargo:rerun-if-changed={path}");
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory"));
    // A separate target directory lets the browser build run while Cargo owns the server build lock.
    let target = out.join("wasm-target");
    let status = Command::new(env::var_os("CARGO").expect("Cargo executable"))
        .args([
            "build",
            "--locked",
            "-p",
            "rill-client",
            "-p",
            "rill-appearance",
            "--target",
            "wasm32-unknown-unknown",
            "--profile",
            "browser",
            "--target-dir",
        ])
        .arg(&target)
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTFLAGS")
        .status()
        .expect("start Rust browser build");
    assert!(
        status.success(),
        "Browser build failed. Install its Rust target with: rustup target add wasm32-unknown-unknown"
    );
    let assets = out.join("browser");
    fs::create_dir_all(&assets).expect("create browser assets directory");
    let compiled = target.join("wasm32-unknown-unknown/browser");
    Bindgen::new()
        .input_path(compiled.join("rill_client.wasm"))
        .web(true)
        .expect("select browser bindings")
        .typescript(false)
        .generate(&assets)
        .expect("generate WebAssembly browser bindings");
    write_bootstrap(&compiled, &assets);
    for name in ["boot.js", "rill_client.js", "rill_client_bg.wasm"] {
        let mut src = fs::File::open(assets.join(name)).expect("open browser asset");
        let dst = fs::File::create(assets.join(format!("{name}.gz"))).expect("create gzip asset");
        let mut gzip = GzEncoder::new(dst, Compression::best());
        io::copy(&mut src, &mut gzip).expect("compress browser asset");
        gzip.finish().expect("finish gzip asset");
    }
}

fn write_bootstrap(compiled: &Path, assets: &Path) {
    // The classic script must finish synchronously before the first paint. Embed only the
    // preference reader here so first paint does not wait for the larger chat module.
    let mut module = Bindgen::new()
        .input_path(compiled.join("rill_appearance.wasm"))
        .no_modules(true)
        .expect("classic script bindings")
        .no_modules_global("RillAppearance")
        .expect("appearance module name")
        .typescript(false)
        .generate_output()
        .expect("generate appearance bindings");
    let bytes = STANDARD.encode(module.wasm_mut().emit_wasm());
    let loader = include_str!("../client/boot.js").replace("__RILL_APPEARANCE_WASM__", &bytes);
    fs::write(assets.join("boot.js"), format!("{}\n{loader}", module.js()))
        .expect("write browser bootstrap");
}
