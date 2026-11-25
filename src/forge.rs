//! Forge and NeoForge: download the official installer, run its processors
//! (which patch the game) and return the launch profile.

use crate::loaders::Loader;
use crate::mojang::{self, maven_path, Library, VersionJson};
use crate::net::{self, Download};
use crate::paths;
use crate::tasks::{log, TaskRef};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const FORGE_MAVEN: &str = "https://maven.minecraftforge.net/net/minecraftforge/forge";
const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases/net/neoforged/neoforge";
const CP_SEP: &str = if cfg!(target_os = "windows") {
    ";"
} else {
    ":"
};

/// Recommended (or newest) Forge version for a Minecraft version.
pub fn latest_forge(mc: &str) -> Result<String> {
    let v: Value = net::get_json(
        "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json",
    )?;
    let promos = &v["promos"];
    promos[format!("{mc}-recommended")]
        .as_str()
        .or_else(|| promos[format!("{mc}-latest")].as_str())
        .map(String::from)
        .with_context(|| format!("Forge doesn't support Minecraft {mc}"))
}

/// NeoForge versions start with the Minecraft version minus the leading
/// "1." (1.21.1 → 21.1.x; 26.3 → 26.3.0.x).
fn neoforge_prefix(mc: &str) -> String {
    let parts: Vec<&str> = mc.split('.').collect();
    if parts.first() == Some(&"1") {
        format!(
            "{}.{}.",
            parts.get(1).unwrap_or(&"0"),
            parts.get(2).unwrap_or(&"0")
        )
    } else {
        format!(
            "{}.{}.{}.",
            parts[0],
            parts.get(1).unwrap_or(&"0"),
            parts.get(2).unwrap_or(&"0")
        )
    }
}

pub fn latest_neoforge(mc: &str) -> Result<String> {
    #[derive(Deserialize)]
    struct List {
        versions: Vec<String>,
    }
    let list: List = net::get_json(
        "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge",
    )?;
    let prefix = neoforge_prefix(mc);
    let matching: Vec<&String> = list
        .versions
        .iter()
        .filter(|v| v.starts_with(&prefix))
        .collect();
    matching
        .iter()
        .rev()
        .find(|v| !v.contains("beta") && !v.contains("alpha"))
        .or(matching.last())
        .map(|v| v.to_string())
        .with_context(|| format!("NeoForge doesn't support Minecraft {mc}"))
}

fn installer_url(loader: Loader, mc: &str, version: &str) -> String {
    match loader {
        Loader::NeoForge => format!("{NEOFORGE_MAVEN}/{version}/neoforge-{version}-installer.jar"),
        _ => format!("{FORGE_MAVEN}/{mc}-{version}/forge-{mc}-{version}-installer.jar"),
    }
}

fn lib_path(coord: &str) -> Result<PathBuf> {
    Ok(paths::libraries()
        .join(maven_path(coord).with_context(|| format!("bad library name {coord}"))?))
}

fn read_zip_string(zip: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Result<String> {
    let mut s = String::new();
    zip.by_name(name)
        .with_context(|| format!("installer has no {name}"))?
        .read_to_string(&mut s)?;
    Ok(s)
}

fn main_class(jar: &Path) -> Result<String> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(jar)?)?;
    let manifest = read_zip_string(&mut zip, "META-INF/MANIFEST.MF")?;
    manifest
        .lines()
        .find_map(|l| l.strip_prefix("Main-Class:"))
        .map(|s| s.trim().to_string())
        .with_context(|| format!("{} has no Main-Class", jar.display()))
}

#[derive(Deserialize)]
struct Processor {
    jar: String,
    #[serde(default)]
    classpath: Vec<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    sides: Option<Vec<String>>,
}

/// Make sure Forge/NeoForge is installed and return its profile (to be
/// merged on top of vanilla).
pub fn install(
    loader: Loader,
    mc: &str,
    version: &str,
    client_jar: &Path,
    java: &Path,
    task: &TaskRef,
) -> Result<VersionJson> {
    let tag = format!("{}-{mc}-{version}", loader.label().to_lowercase());
    let dir = paths::versions().join(&tag);
    let profile_path = dir.join("profile.json");
    let marker = dir.join(".paprika-installed");
    if marker.exists() {
        if let Ok(v) = serde_json::from_str::<VersionJson>(&std::fs::read_to_string(&profile_path)?)
        {
            return Ok(v);
        }
    }
    std::fs::create_dir_all(&dir)?;

    task.status(format!(
        "Downloading {} {version} installer",
        loader.label()
    ));
    let installer = dir.join("installer.jar");
    net::download(&Download::new(
        installer_url(loader, mc, version),
        &installer,
    ))?;
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&installer)?)?;
    let profile: Value = serde_json::from_str(&read_zip_string(&mut zip, "install_profile.json")?)?;
    if profile.get("install").is_some() {
        bail!("Forge for Minecraft older than 1.13 isn't supported");
    }
    let version_file = profile["json"]
        .as_str()
        .unwrap_or("/version.json")
        .trim_start_matches('/')
        .to_string();
    let version_text = read_zip_string(&mut zip, &version_file)?;
    let version_json: VersionJson =
        serde_json::from_str(&version_text).context("parsing loader version.json")?;

    // Libraries bundled inside the installer.
    task.status("Unpacking installer");
    for i in 0..zip.len() {
        let mut e = zip.by_index(i)?;
        let Some(name) = e.enclosed_name() else {
            continue;
        };
        let Ok(rel) = name.strip_prefix("maven") else {
            continue;
        };
        if e.is_dir() || rel.as_os_str().is_empty() {
            continue;
        }
        let out = paths::libraries().join(rel);
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::io::copy(&mut e, &mut std::fs::File::create(out)?)?;
    }

    // Libraries for the processors and for the game itself.
    let tool_libs: Vec<Library> =
        serde_json::from_value(profile["libraries"].clone()).unwrap_or_default();
    let mut all = VersionJson {
        libraries: tool_libs,
        ..Default::default()
    };
    all.libraries.extend(version_json.libraries.iter().cloned());
    let plan = mojang::plan_libraries(&all);
    task.status(format!("Downloading {} libraries", loader.label()));
    net::download_all(plan.downloads, task)?;
    task.set_progress(0, 0);

    // Data values the processors refer to as {KEY}.
    let tmp = dir.join("tmp");
    std::fs::create_dir_all(&tmp)?;
    let mut data: HashMap<String, String> = HashMap::new();
    if let Some(obj) = profile["data"].as_object() {
        for (k, v) in obj {
            let Some(val) = v["client"].as_str() else {
                continue;
            };
            let resolved = if let Some(coord) =
                val.strip_prefix('[').and_then(|s| s.strip_suffix(']'))
            {
                lib_path(coord)?.display().to_string()
            } else if let Some(lit) = val.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
                lit.to_string()
            } else if let Some(inner) = val.strip_prefix('/') {
                let out = tmp.join(inner);
                if let Some(p) = out.parent() {
                    std::fs::create_dir_all(p)?;
                }
                let mut e = zip
                    .by_name(inner)
                    .with_context(|| format!("installer has no {inner}"))?;
                std::io::copy(&mut e, &mut std::fs::File::create(&out)?)?;
                out.display().to_string()
            } else {
                val.to_string()
            };
            data.insert(k.clone(), resolved);
        }
    }
    data.insert("SIDE".into(), "client".into());
    data.insert("MINECRAFT_JAR".into(), client_jar.display().to_string());
    data.insert("MINECRAFT_VERSION".into(), mc.to_string());
    data.insert("ROOT".into(), tmp.display().to_string());
    data.insert("INSTALLER".into(), installer.display().to_string());
    data.insert(
        "LIBRARY_DIR".into(),
        paths::libraries().display().to_string(),
    );

    let substitute = |arg: &str| -> Result<String> {
        if let Some(coord) = arg.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            return Ok(lib_path(coord)?.display().to_string());
        }
        let mut out = arg.to_string();
        for (k, v) in &data {
            out = out.replace(&format!("{{{k}}}"), v);
        }
        Ok(out)
    };

    let processors: Vec<Processor> =
        serde_json::from_value(profile["processors"].clone()).unwrap_or_default();
    let client: Vec<&Processor> = processors
        .iter()
        .filter(|p| {
            p.sides
                .as_ref()
                .is_none_or(|s| s.iter().any(|x| x == "client"))
        })
        .collect();
    for (i, p) in client.iter().enumerate() {
        task.status(format!(
            "Installing {} ({}/{})",
            loader.label(),
            i + 1,
            client.len()
        ));
        task.set_progress(i, client.len());
        let jar = lib_path(&p.jar)?;
        let mut cp = vec![jar.display().to_string()];
        for c in &p.classpath {
            cp.push(lib_path(c)?.display().to_string());
        }
        let args: Vec<String> = p
            .args
            .iter()
            .map(|a| substitute(a))
            .collect::<Result<_>>()?;
        let mut cmd = Command::new(java);
        cmd.arg("-cp")
            .arg(cp.join(CP_SEP))
            .arg(main_class(&jar)?)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let mut child = cmd
            .spawn()
            .with_context(|| format!("couldn't start Java at {}", java.display()))?;
        let err = child.stderr.take().unwrap();
        let err_thread = std::thread::spawn(move || {
            BufReader::new(err)
                .lines()
                .map_while(Result::ok)
                .collect::<Vec<_>>()
        });
        for line in BufReader::new(child.stdout.take().unwrap())
            .lines()
            .map_while(Result::ok)
        {
            log(format!("[installer] {line}"));
        }
        let errors = err_thread.join().unwrap_or_default();
        for line in &errors {
            log(format!("[installer] {line}"));
        }
        let status = child.wait()?;
        if !status.success() {
            bail!(
                "{} installer step {} ({}) failed - see Console",
                loader.label(),
                i + 1,
                p.jar
            );
        }
    }
    task.set_progress(0, 0);

    let _ = std::fs::remove_dir_all(&tmp);
    let _ = std::fs::remove_file(&installer);
    std::fs::write(&profile_path, version_text)?;
    std::fs::write(&marker, b"ok")?;
    log(format!(
        "[paprika] Installed {} {version} for Minecraft {mc}",
        loader.label()
    ));
    Ok(version_json)
}

#[cfg(test)]
mod tests {
    #[test]
    fn neoforge_prefixes() {
        assert_eq!(super::neoforge_prefix("1.21.1"), "21.1.");
        assert_eq!(super::neoforge_prefix("1.21"), "21.0.");
        assert_eq!(super::neoforge_prefix("26.3"), "26.3.0.");
        assert_eq!(super::neoforge_prefix("26.1.2"), "26.1.2.");
    }
}
