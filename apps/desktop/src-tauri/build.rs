fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Tauri embeds its Windows manifest only into the app binary. Test binaries that link tauri
    // then fail with STATUS_ENTRYPOINT_NOT_FOUND (comctl32 v6 is missing), so embed our own
    // manifest into every target instead.
    let target_windows_msvc = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    let mut windows = tauri_build::WindowsAttributes::new();
    if target_windows_msvc {
        windows = tauri_build::WindowsAttributes::new_without_app_manifest();
        let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR")?)
            .join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))?;
    bundle_plugins()?;
    Ok(())
}

/// Packs the plugins shipped with the app (`plugins/`, ADR-0023/0030) into `OUT_DIR`, where
/// `src/bundled.rs` includes them. Packing is deterministic, so the packages' SHA-256 only change
/// with their sources.
fn bundle_plugins() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR")?).join("../../../plugins");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR")?);
    for dir in ["system-srd5", "system-generic-d6", "theme-midnight"] {
        let source = root.join(dir);
        println!("cargo:rerun-if-changed={}", source.display());
        for sub in ["sheet", "compendia"] {
            println!("cargo:rerun-if-changed={}", source.join(sub).display());
        }
        let bytes = dnd_plugin::pack_directory(&source)?;
        std::fs::write(out.join(format!("{dir}.dndplugin")), bytes)?;
    }
    Ok(())
}
