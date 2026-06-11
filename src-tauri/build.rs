fn main() {
    // Suppress tauri-build's default app manifest (embedded only into the
    // app binary via rustc-link-arg-bins) so we can embed the same manifest
    // into EVERY Windows link target below — test executables included.
    // Without a manifest, test binaries bind Common-Controls v5 and crash
    // at startup with STATUS_ENTRYPOINT_NOT_FOUND before running any test.
    // https://github.com/tauri-apps/tauri/pull/4383#issuecomment-1212221864
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest()),
    )
    .expect("failed to run tauri-build");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV");
    if target_os == "windows" && target_env.as_deref() == Ok("msvc") {
        let manifest = std::env::current_dir()
            .unwrap()
            .join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg=/MANIFESTINPUT:{}",
            manifest.to_str().unwrap()
        );
    }
}
