//! Fabric and Quilt support via their meta servers.

use crate::mojang::VersionJson;
use crate::net;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Loader {
    #[default]
    Vanilla,
    Fabric,
    Quilt,
    Forge,
    NeoForge,
}

impl Loader {
    pub const ALL: [Loader; 5] = [
        Loader::Vanilla,
        Loader::Fabric,
        Loader::Quilt,
        Loader::Forge,
        Loader::NeoForge,
    ];

    /// Forge-style loaders need their installer run before launching.
    pub fn is_forge_like(self) -> bool {
        matches!(self, Loader::Forge | Loader::NeoForge)
    }

    pub fn label(self) -> &'static str {
        match self {
            Loader::Vanilla => "Vanilla",
            Loader::Fabric => "Fabric",
            Loader::Quilt => "Quilt",
            Loader::Forge => "Forge",
            Loader::NeoForge => "NeoForge",
        }
    }

    /// Modrinth loader ids this instance can run. Quilt runs Fabric mods.
    pub fn modrinth_ids(self) -> Vec<String> {
        match self {
            Loader::Vanilla => vec![],
            Loader::Fabric => vec!["fabric".into()],
            Loader::Quilt => vec!["quilt".into(), "fabric".into()],
            Loader::Forge => vec!["forge".into()],
            Loader::NeoForge => vec!["neoforge".into()],
        }
    }

    fn meta(self) -> Option<&'static str> {
        match self {
            Loader::Vanilla | Loader::Forge | Loader::NeoForge => None,
            Loader::Fabric => Some("https://meta.fabricmc.net/v2"),
            Loader::Quilt => Some("https://meta.quiltmc.org/v3"),
        }
    }
}

#[derive(Deserialize)]
struct LoaderEntry {
    loader: LoaderInfo,
}

#[derive(Deserialize)]
struct LoaderInfo {
    version: String,
    #[serde(default)]
    stable: Option<bool>,
}

/// Newest stable loader version for this Minecraft version.
pub fn latest_loader(loader: Loader, mc: &str) -> Result<String> {
    match loader {
        Loader::Forge => return crate::forge::latest_forge(mc),
        Loader::NeoForge => return crate::forge::latest_neoforge(mc),
        _ => {}
    }
    let Some(base) = loader.meta() else {
        bail!("vanilla has no loader")
    };
    let list: Vec<LoaderEntry> = net::get_json(&format!("{base}/versions/loader/{mc}"))?;
    if list.is_empty() {
        bail!("{} doesn't support Minecraft {mc} yet", loader.label());
    }
    let pick = list
        .iter()
        .find(|e| {
            e.loader
                .stable
                .unwrap_or_else(|| !e.loader.version.contains("beta"))
        })
        .unwrap_or(&list[0]);
    Ok(pick.loader.version.clone())
}

pub fn profile(loader: Loader, mc: &str, version: &str) -> Result<VersionJson> {
    let Some(base) = loader.meta() else {
        bail!("vanilla has no loader")
    };
    let url = format!("{base}/versions/loader/{mc}/{version}/profile/json");
    let text = net::get_string(&url)?;
    let path =
        crate::paths::versions().join(format!("{}-{mc}-{version}", loader.label().to_lowercase()));
    std::fs::create_dir_all(&path)?;
    std::fs::write(path.join("profile.json"), &text)?;
    serde_json::from_str(&text).context("parsing loader profile")
}
