//! Java runtimes: Paprika downloads the exact one each Minecraft version wants
//! from Mojang, so players never need to install Java themselves.

use crate::net::{self, Download};
use crate::paths;
use crate::tasks::TaskRef;
use anyhow::{Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

const RUNTIMES_URL: &str =
    "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

fn platform() -> &'static str {
    if cfg!(target_os = "windows") {
        if cfg!(target_arch = "aarch64") {
            "windows-arm64"
        } else if cfg!(target_arch = "x86") {
            "windows-x86"
        } else {
            "windows-x64"
        }
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "mac-os-arm64"
        } else {
            "mac-os"
        }
    } else if cfg!(target_arch = "x86") {
        "linux-i386"
    } else {
        "linux"
    }
}

pub fn java_bin(home: &Path) -> PathBuf {
    if cfg!(target_os = "windows") {
        home.join("bin").join("javaw.exe")
    } else {
        home.join("bin").join("java")
    }
}

/// Java on the PATH, used when Mojang has no runtime for this platform.
pub fn system_java() -> PathBuf {
    PathBuf::from(if cfg!(target_os = "windows") {
        "javaw"
    } else {
        "java"
    })
}

pub fn ensure_runtime(component: &str, task: &TaskRef) -> Result<PathBuf> {
    let home = paths::java().join(component);
    let marker = home.join(".paprika-complete");
    if marker.exists() && java_bin(&home).exists() {
        return Ok(java_bin(&home));
    }
    task.status(format!("Fetching Java runtime ({component})"));
    let all: Value = net::get_json(RUNTIMES_URL)?;
    let Some(entry) = all[platform()][component]
        .as_array()
        .and_then(|a| a.first())
    else {
        crate::tasks::log(format!(
            "[paprika] No Mojang Java '{component}' for {}, using system Java",
            platform()
        ));
        return Ok(system_java());
    };
    let manifest_url = entry["manifest"]["url"]
        .as_str()
        .context("runtime manifest url")?;
    let manifest: Value = net::get_json(manifest_url)?;
    let files = manifest["files"].as_object().context("runtime file list")?;
    let mut downloads = Vec::new();
    let mut links = Vec::new();
    for (rel, f) in files {
        let p = home.join(rel);
        match f["type"].as_str() {
            Some("directory") => std::fs::create_dir_all(&p)?,
            Some("file") => {
                let raw = &f["downloads"]["raw"];
                let Some(url) = raw["url"].as_str() else {
                    continue;
                };
                let mut d = Download::new(url, &p)
                    .sha1(raw["sha1"].as_str().map(String::from))
                    .size(raw["size"].as_u64());
                d.executable = f["executable"].as_bool().unwrap_or(false);
                downloads.push(d);
            }
            Some("link") => {
                if let Some(t) = f["target"].as_str() {
                    links.push((p, t.to_string()));
                }
            }
            _ => {}
        }
    }
    task.status(format!("Downloading Java ({component})"));
    net::download_all(downloads, task)?;
    #[cfg(unix)]
    for (p, target) in links {
        let _ = std::fs::remove_file(&p);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let _ = std::os::unix::fs::symlink(target, &p);
    }
    #[cfg(not(unix))]
    drop(links);
    std::fs::write(&marker, b"ok")?;
    Ok(java_bin(&home))
}
