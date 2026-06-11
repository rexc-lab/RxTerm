fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV");
    let is_windows_msvc = target_os == "windows" && target_env.as_deref() == Ok("msvc");

    // On MSVC, suppress tauri-build's default app manifest (embedded only
    // into the app binary via rustc-link-arg-bins) and instead embed the
    // same manifest into EVERY link target below — test executables
    // included. Without a manifest, test binaries bind Common-Controls v5
    // and crash at startup with STATUS_ENTRYPOINT_NOT_FOUND before running
    // any test. Other targets (incl. windows-gnu) keep tauri-build's
    // default manifest handling, since the link args below are MSVC-only.
    // https://github.com/tauri-apps/tauri/pull/4383#issuecomment-1212221864
    let attributes = if is_windows_msvc {
        tauri_build::Attributes::new()
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest())
    } else {
        tauri_build::Attributes::new()
    };
    tauri_build::try_build(attributes).expect("failed to run tauri-build");

    if is_windows_msvc {
        let manifest = std::env::current_dir()
            .unwrap()
            .join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg=/MANIFESTINPUT:{}",
            manifest.to_str().unwrap()
        );
        // Without this, link.exe merges a default asInvoker <trustInfo>
        // fragment into the manifest; the old .rc path embedded the XML
        // verbatim, and a future <trustInfo> added to the XML would
        // conflict with the linker's fragment at merge time.
        println!("cargo:rustc-link-arg=/MANIFESTUAC:NO");
    }
}
