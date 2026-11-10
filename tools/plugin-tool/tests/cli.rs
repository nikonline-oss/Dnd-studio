//! The CLI is the catalog pipeline's entry point (roadmap R3.1): what it accepts, the app accepts,
//! and what it packs is byte-identical to the library.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_plugins() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins")
}

fn tool() -> Command {
    Command::new(env!("CARGO_BIN_EXE_plugin-tool"))
}

#[test]
fn every_bundled_plugin_passes_validation() {
    for name in ["system-srd5", "system-generic-d6", "theme-midnight"] {
        let dir = repo_plugins().join(name);
        let output = tool().arg("validate").arg(&dir).output().unwrap();
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains(": ok"), "{stdout}");
    }
}

#[test]
fn packing_is_reproducible_and_matches_the_library() {
    let dir = repo_plugins().join("system-generic-d6");
    let out_dir = tempfile::tempdir().unwrap();
    let first = out_dir.path().join("first.dndplugin");
    let second = out_dir.path().join("second.dndplugin");
    for target in [&first, &second] {
        let output = tool()
            .arg("pack")
            .arg(&dir)
            .arg("-o")
            .arg(target)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let first = std::fs::read(&first).unwrap();
    assert_eq!(&first, &std::fs::read(&second).unwrap());
    assert_eq!(first, dnd_plugin::pack_directory(&dir).unwrap());
}

#[test]
fn an_invalid_package_fails_with_a_diagnostic() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("manifest.json"), "{ not json").unwrap();
    let output = tool().arg("validate").arg(dir.path()).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid"), "{stderr}");
}

#[test]
fn usage_errors_are_reported() {
    let output = tool().arg("validate").output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("at least one"));
    let output = tool().arg("nonsense").output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown command"));
}

// ---------- Signing and the index (ADR-0060) ----------

fn public_key_hex(private_key: &Path) -> String {
    use ring::signature::KeyPair as _;
    let bytes = std::fs::read(private_key).unwrap();
    let pair = ring::signature::Ed25519KeyPair::from_pkcs8(&bytes).unwrap();
    dnd_plugin::signature::encode_hex(pair.public_key().as_ref())
}

#[test]
fn a_generated_key_signs_a_package_that_verifies() {
    let dir = tempfile::tempdir().unwrap();
    let key = dir.path().join("key.pkcs8");
    let output = tool().arg("keygen").arg("-o").arg(&key).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // The key must not be printed; only the public half is.
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("private key: ") || stdout.starts_with("private key: "));
    let public = public_key_hex(&key);

    let package = dir.path().join("generic.dndplugin");
    tool()
        .arg("pack")
        .arg(repo_plugins().join("system-generic-d6"))
        .arg("-o")
        .arg(&package)
        .output()
        .unwrap();
    let output = tool()
        .arg("sign")
        .arg("--key")
        .arg(&key)
        .arg("--package")
        .arg(&package)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let signed: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("sign prints JSON");
    let public_bytes = {
        let mut bytes = [0u8; 32];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&public[index * 2..index * 2 + 2], 16).unwrap();
        }
        bytes
    };
    dnd_plugin::signature::verify_plugin(
        &public_bytes,
        signed["id"].as_str().unwrap(),
        signed["version"].as_str().unwrap(),
        signed["sha256"].as_str().unwrap(),
        signed["signature"].as_str().unwrap(),
    )
    .unwrap();
}

#[test]
fn keygen_refuses_to_overwrite_an_existing_key() {
    let dir = tempfile::tempdir().unwrap();
    let key = dir.path().join("key.pkcs8");
    std::fs::write(&key, b"existing").unwrap();
    let output = tool().arg("keygen").arg("-o").arg(&key).output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("refusing to overwrite"));
    assert_eq!(std::fs::read(&key).unwrap(), b"existing");
}

#[test]
fn the_index_is_signed_and_lists_immutable_versions() {
    let dir = tempfile::tempdir().unwrap();
    let key = dir.path().join("key.pkcs8");
    tool().arg("keygen").arg("-o").arg(&key).output().unwrap();
    let package = dir.path().join("generic.dndplugin");
    tool()
        .arg("pack")
        .arg(repo_plugins().join("system-generic-d6"))
        .arg("-o")
        .arg(&package)
        .output()
        .unwrap();
    let index_path = dir.path().join("index.json");
    let output = tool()
        .arg("index")
        .arg("--key")
        .arg(&key)
        .arg("--base-url")
        .arg("https://example.org/plugins")
        .arg("--out")
        .arg(&index_path)
        .arg(&package)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let bytes = std::fs::read(&index_path).unwrap();
    let signature = std::fs::read_to_string(index_path.with_extension("sig")).unwrap();
    let public = public_key_hex(&key);
    let mut public_bytes = [0u8; 32];
    for (index, byte) in public_bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&public[index * 2..index * 2 + 2], 16).unwrap();
    }
    dnd_plugin::signature::verify_index(&public_bytes, &bytes, signature.trim()).unwrap();
    // Any edit of the file breaks the signature.
    let mut tampered = bytes.clone();
    let position = tampered.len() / 2;
    tampered[position] ^= 0x01;
    assert!(
        dnd_plugin::signature::verify_index(&public_bytes, &tampered, signature.trim()).is_err()
    );

    let index: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(index["format"], 1);
    let plugins = index["plugins"].as_array().unwrap();
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0]["id"], "generic-d6");
    assert_eq!(plugins[0]["revoked"], false);
    assert_eq!(
        plugins[0]["archive_url"],
        "https://example.org/plugins/generic.dndplugin"
    );
}

/// The publishing pipeline (roadmap R3.5) as exercised locally: pack every bundled plugin, sign the
/// index, then read it back the way the client does and install one entry's package. No network and
/// no GitHub — this proves the pieces fit, not that the external repository is configured.
#[test]
fn the_published_catalog_is_accepted_by_the_client() {
    let dir = tempfile::tempdir().unwrap();
    let key = dir.path().join("key.pkcs8");
    tool().arg("keygen").arg("-o").arg(&key).output().unwrap();

    let mut packages = Vec::new();
    for name in ["system-generic-d6", "theme-midnight"] {
        let out = dir.path().join(format!("{name}.dndplugin"));
        tool()
            .arg("pack")
            .arg(repo_plugins().join(name))
            .arg("-o")
            .arg(&out)
            .output()
            .unwrap();
        packages.push(out);
    }
    let index_path = dir.path().join("index.json");
    let mut index_cmd = tool();
    index_cmd
        .arg("index")
        .arg("--key")
        .arg(&key)
        .arg("--base-url")
        .arg("https://example.org/plugins")
        .arg("--out")
        .arg(&index_path);
    for package in &packages {
        index_cmd.arg(package);
    }
    let output = index_cmd.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let public = public_key_hex(&key);
    let mut public_bytes = [0u8; 32];
    for (index, byte) in public_bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&public[index * 2..index * 2 + 2], 16).unwrap();
    }
    let bytes = std::fs::read(&index_path).unwrap();
    let signature = std::fs::read_to_string(index_path.with_extension("sig")).unwrap();
    let catalog = dnd_plugin::catalog::read_index(&bytes, &public_bytes, signature.trim()).unwrap();
    assert_eq!(catalog.plugins.len(), 2);

    // The client installs one entry: the package from the catalog must match the signed record.
    let entry = catalog
        .installable(
            "generic-d6",
            &dnd_plugin::semver::Version::parse("0.2.0").unwrap(),
        )
        .unwrap();
    let package = std::fs::read(dir.path().join("system-generic-d6.dndplugin")).unwrap();
    let verified = entry.verify_package(&package, &public_bytes).unwrap();
    assert_eq!(verified.manifest.id, "generic-d6");

    // A package that is not the one the index signed is refused.
    let other = std::fs::read(dir.path().join("theme-midnight.dndplugin")).unwrap();
    assert!(entry.verify_package(&other, &public_bytes).is_err());
}
