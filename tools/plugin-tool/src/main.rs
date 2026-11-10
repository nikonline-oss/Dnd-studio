//! Plugin CLI for authors and the catalog pipeline (ADR-0006, roadmap R3.1).
//!
//! ```text
//! plugin-tool validate <dir-or-.dndplugin>...
//! plugin-tool pack <dir> [-o <file.dndplugin>]
//! ```
//!
//! `validate` uses the same loader as the application: a package that passes here opens there.
//! `pack` uses the same writer, so the archive it produces is byte-identical to the one the app
//! builds from a source folder — the repository's own packages are checked by that property.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use dnd_plugin::{PackageLimits, PluginPackage, pack_directory, read_package_file};
use ring::signature::KeyPair as _;

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<()> {
    let Some(command) = args.first().map(String::as_str) else {
        print_help();
        return Ok(());
    };
    match command {
        "validate" => {
            let targets = &args[1..];
            if targets.is_empty() {
                bail!("validate needs at least one directory or .dndplugin file");
            }
            let mut failed = 0;
            for target in targets {
                match validate(Path::new(target)) {
                    Ok(package) => print_summary(&package, target),
                    Err(error) => {
                        failed += 1;
                        eprintln!("{target}: invalid: {error:#}");
                    }
                }
            }
            if failed > 0 {
                bail!("{failed} of {} packages are invalid", targets.len());
            }
            Ok(())
        }
        "pack" => pack(&args[1..]),
        "keygen" => keygen(&args[1..]),
        "sign" => sign(&args[1..]),
        "index" => build_index(&args[1..]),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => bail!("unknown command {other:?}; try `plugin-tool help`"),
    }
}

fn print_help() {
    println!(
        "plugin-tool {}\n\
         \n\
         usage:\n\
         \x20 plugin-tool validate <dir-or-.dndplugin>...   check manifest, sheets, deps\n\
         \x20 plugin-tool pack <dir> [-o <out.dndplugin>]    build a package\n\
         \x20 plugin-tool keygen -o <key.pkcs8>                new Ed25519 signing key\n\
         \x20 plugin-tool sign --key <pkcs8> --package <file>  signature for one package\n\
         \x20 plugin-tool index --key <pkcs8> --base-url <url> --out <index.json> <packages...>\n\
         \x20 plugin-tool help                              this text\n",
        env!("CARGO_PKG_VERSION")
    );
}

fn validate(path: &Path) -> Result<PluginPackage> {
    if path.is_dir() {
        let bytes = pack_directory(path).with_context(|| format!("packing {}", path.display()))?;
        return dnd_plugin::read_package(&bytes, PackageLimits::default())
            .with_context(|| format!("validating {}", path.display()));
    }
    read_package_file(path, PackageLimits::default())
        .map(|(package, _)| package)
        .with_context(|| format!("validating {}", path.display()))
}

fn print_summary(package: &PluginPackage, target: &str) {
    let manifest = &package.manifest;
    println!("{target}: ok");
    println!("  id:       {}", manifest.id);
    println!("  version:  {}", manifest.version);
    println!("  kind:     {}", manifest.kind.as_str());
    println!("  license:  {}", manifest.license);
    println!("  compat:   {}", manifest.app_compat);
    println!("  sha256:   {}", package.sha256);
    if package.system.is_some() {
        println!("  rules:    system plugin");
    }
    println!(
        "  files:    {} compendium(s), {} trigger template(s), {} asset(s)",
        package.compendia.len(),
        package.trigger_templates.len(),
        package.assets.len()
    );
    if !manifest.dependencies.is_empty() {
        let deps: Vec<_> = manifest
            .dependencies
            .iter()
            .map(|(id, range)| format!("{id} {range}"))
            .collect();
        println!("  depends:  {}", deps.join(", "));
    }
}

fn pack(args: &[String]) -> Result<()> {
    let mut dir: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-o" | "--out" => {
                index += 1;
                let value = args.get(index).context("-o needs a file name after it")?;
                out = Some(PathBuf::from(value));
            }
            other if dir.is_none() => dir = Some(PathBuf::from(other)),
            other => bail!("unexpected argument {other:?}"),
        }
        index += 1;
    }
    let dir = dir.context("pack needs a source directory")?;
    let bytes = pack_directory(&dir).with_context(|| format!("packing {}", dir.display()))?;
    // Validate what was just packed: a package the app would refuse must not be produced.
    let package = dnd_plugin::read_package(&bytes, PackageLimits::default())
        .with_context(|| format!("validating the packed {}", dir.display()))?;
    let out = out.unwrap_or_else(|| {
        let name = format!(
            "{}-{}.dndplugin",
            package.manifest.id, package.manifest.version
        );
        dir.parent().unwrap_or(Path::new(".")).join(name)
    });
    std::fs::write(&out, &bytes).with_context(|| format!("writing {}", out.display()))?;
    println!(
        "{} ({} bytes, sha256 {})",
        out.display(),
        bytes.len(),
        package.sha256
    );
    Ok(())
}

// ---------- Signing (ADR-0060) ----------

fn keygen(args: &[String]) -> Result<()> {
    let mut out: Option<PathBuf> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-o" | "--out" => {
                index += 1;
                out = Some(PathBuf::from(
                    args.get(index).context("-o needs a file name after it")?,
                ));
            }
            other => bail!("unexpected argument {other:?}"),
        }
        index += 1;
    }
    let out = out.context("keygen needs -o <key.pkcs8>: the private key must not go to stdout")?;
    if out.exists() {
        bail!(
            "{} already exists; refusing to overwrite a signing key",
            out.display()
        );
    }
    let document =
        ring::signature::Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new())
            .map_err(|_| anyhow::anyhow!("the OS random source is unavailable"))?;
    std::fs::write(&out, document.as_ref())
        .with_context(|| format!("writing {}", out.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&out, std::fs::Permissions::from_mode(0o600))?;
    }
    let pair = ring::signature::Ed25519KeyPair::from_pkcs8(document.as_ref())
        .map_err(|_| anyhow::anyhow!("the generated key does not parse"))?;
    println!("private key: {}", out.display());
    println!(
        "public key:  {}",
        dnd_plugin::signature::encode_hex(pair.public_key().as_ref())
    );
    println!("Keep the private key in CI secrets; the public key goes into the app's trust list.");
    Ok(())
}

fn signing_key(path: &Path) -> Result<ring::signature::Ed25519KeyPair> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    ring::signature::Ed25519KeyPair::from_pkcs8(&bytes).map_err(|_| {
        anyhow::anyhow!(
            "{} is not an unencrypted Ed25519 PKCS#8 key",
            path.display()
        )
    })
}

fn sign(args: &[String]) -> Result<()> {
    let mut key: Option<PathBuf> = None;
    let mut package: Option<PathBuf> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--key" => {
                index += 1;
                key = Some(PathBuf::from(
                    args.get(index).context("--key needs a file after it")?,
                ));
            }
            "--package" => {
                index += 1;
                package = Some(PathBuf::from(
                    args.get(index).context("--package needs a file after it")?,
                ));
            }
            other => bail!("unexpected argument {other:?}"),
        }
        index += 1;
    }
    let key = signing_key(&key.context("sign needs --key <pkcs8>")?)?;
    let package = package.context("sign needs --package <file.dndplugin>")?;
    let (read, _) = read_package_file(&package, PackageLimits::default())
        .with_context(|| format!("reading {}", package.display()))?;
    let manifest = &read.manifest;
    let version = manifest.version.to_string();
    let transcript = dnd_plugin::signature::plugin_transcript(&manifest.id, &version, &read.sha256);
    let signature = dnd_plugin::signature::encode_hex(key.sign(&transcript).as_ref());
    println!(
        "{}",
        serde_json::json!({
            "id": manifest.id,
            "version": manifest.version,
            "sha256": read.sha256,
            "signature": signature,
        })
    );
    Ok(())
}

fn build_index(args: &[String]) -> Result<()> {
    let mut key: Option<PathBuf> = None;
    let mut base_url: Option<String> = None;
    let mut out: Option<PathBuf> = None;
    let mut packages: Vec<PathBuf> = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--key" => {
                index += 1;
                key = Some(PathBuf::from(
                    args.get(index).context("--key needs a file after it")?,
                ));
            }
            "--base-url" => {
                index += 1;
                base_url = Some(
                    args.get(index)
                        .context("--base-url needs a URL after it")?
                        .trim_end_matches('/')
                        .to_owned(),
                );
            }
            "--out" => {
                index += 1;
                out = Some(PathBuf::from(
                    args.get(index).context("--out needs a file after it")?,
                ));
            }
            other => packages.push(PathBuf::from(other)),
        }
        index += 1;
    }
    let key = signing_key(&key.context("index needs --key <pkcs8>")?)?;
    let base_url = base_url.context("index needs --base-url <url>")?;
    let out = out.unwrap_or_else(|| PathBuf::from("index.json"));
    if packages.is_empty() {
        bail!("index needs at least one .dndplugin file");
    }
    let mut entries = Vec::new();
    let mut seen: std::collections::BTreeMap<(String, String), String> =
        std::collections::BTreeMap::new();
    for path in &packages {
        let (package, _) = read_package_file(path, PackageLimits::default())
            .with_context(|| format!("reading {}", path.display()))?;
        let manifest = &package.manifest;
        let entry_key = (manifest.id.clone(), manifest.version.to_string());
        // An immutable version: the same id and version twice with different bytes is an error,
        // not an update (ADR-0006). The same file listed twice is simply one entry.
        match seen.get(&entry_key) {
            Some(hash) if *hash != package.sha256 => bail!(
                "{} {} is already published with another hash",
                manifest.id,
                manifest.version
            ),
            Some(_) => continue,
            None => {
                seen.insert(entry_key, package.sha256.clone());
            }
        }
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .context("a package file name must be valid UTF-8")?;
        let version = manifest.version.to_string();
        let transcript =
            dnd_plugin::signature::plugin_transcript(&manifest.id, &version, &package.sha256);
        let signature = dnd_plugin::signature::encode_hex(key.sign(&transcript).as_ref());
        entries.push(serde_json::json!({
            "id": manifest.id,
            "version": manifest.version,
            "kind": manifest.kind.as_str(),
            "license": manifest.license,
            "app_compat": manifest.app_compat,
            "sha256": package.sha256,
            "archive_url": format!("{base_url}/{file_name}"),
            "signature": signature,
            "revoked": false,
            "dependencies": manifest.dependencies,
        }));
    }
    entries.sort_by(|a, b| {
        let key = |entry: &serde_json::Value| {
            (
                entry["id"].as_str().unwrap_or_default().to_owned(),
                entry["version"].as_str().unwrap_or_default().to_owned(),
            )
        };
        key(a).cmp(&key(b))
    });
    let index = serde_json::json!({
        "format": 1,
        "key_id": "catalog",
        "generated_at": published_at(),
        "plugins": entries,
    });
    let mut bytes = serde_json::to_vec_pretty(&index)?;
    // A text file ends with a newline; the signature covers these exact bytes.
    bytes.push(b'\n');
    std::fs::write(&out, &bytes).with_context(|| format!("writing {}", out.display()))?;
    let signature = dnd_plugin::signature::encode_hex(
        key.sign(&dnd_plugin::signature::index_transcript(&bytes))
            .as_ref(),
    );
    let sig_path = out.with_extension("sig");
    std::fs::write(&sig_path, &signature)
        .with_context(|| format!("writing {}", sig_path.display()))?;
    println!(
        "{} ({} plugins) and {}",
        out.display(),
        index["plugins"].as_array().map_or(0, std::vec::Vec::len),
        sig_path.display()
    );
    Ok(())
}

/// ISO-8601 UTC without pulling a date crate: the catalog workflow only needs a readable stamp.
fn published_at() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let days = seconds / 86_400;
    let (year, month, day) = civil_from_days(i64::try_from(days).unwrap_or(0));
    let rest = seconds % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        (rest % 3600) / 60,
        rest % 60
    )
}

/// Days since 1970-01-01 → (year, month, day), Howard Hinnant's civil-from-days algorithm.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (
        if m <= 2 { y + 1 } else { y },
        u32::try_from(m).unwrap_or(1),
        u32::try_from(d).unwrap_or(1),
    )
}
