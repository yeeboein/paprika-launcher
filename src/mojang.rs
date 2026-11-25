//! Mojang's version metadata: manifest, version JSONs, libraries, assets and
//! the rules that decide which bits apply to this computer.

use crate::net::{self, Download};
use crate::paths;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
pub const LIBRARIES_URL: &str = "https://libraries.minecraft.net/";
pub const RESOURCES_URL: &str = "https://resources.download.minecraft.net/";

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Manifest {
    pub latest: Latest,
    pub versions: Vec<ManifestVersion>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ManifestVersion {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub release_time: String,
}

/// Fetch the version list, falling back to the last cached copy offline.
pub fn fetch_manifest() -> Result<Manifest> {
    let cache = paths::cache().join("version_manifest_v2.json");
    match net::get_string(MANIFEST_URL) {
        Ok(text) => {
            let m: Manifest = serde_json::from_str(&text)?;
            let _ = std::fs::create_dir_all(paths::cache());
            let _ = std::fs::write(&cache, text);
            Ok(m)
        }
        Err(e) => {
            let text = std::fs::read_to_string(&cache).map_err(|_| e)?;
            Ok(serde_json::from_str(&text)?)
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherits_from: Option<String>,
    #[serde(default)]
    pub main_class: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minecraft_arguments: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Arguments>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_index: Option<AssetIndexRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assets: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<Downloads>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_version: Option<JavaVersion>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Arg>,
    #[serde(default)]
    pub jvm: Vec<Arg>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum Arg {
    Plain(String),
    Ruled { rules: Vec<Rule>, value: StrOrVec },
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum StrOrVec {
    One(String),
    Many(Vec<String>),
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Rule {
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<OsRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct OsRule {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct Library {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<LibDownloads>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<Rule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub natives: Option<HashMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct LibDownloads {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<Artifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classifiers: Option<HashMap<String, Artifact>>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Artifact {
    #[serde(default)]
    pub path: Option<String>,
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct AssetIndexRef {
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Downloads {
    #[serde(default)]
    pub client: Option<Artifact>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    pub component: String,
    #[serde(default)]
    pub major_version: u32,
}

pub fn os_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

impl Rule {
    fn matches(&self) -> bool {
        if let Some(os) = &self.os {
            if let Some(name) = &os.name {
                if name != os_name() {
                    return false;
                }
            }
            if let Some(arch) = &os.arch {
                let ours = if cfg!(target_arch = "x86") {
                    "x86"
                } else if cfg!(target_arch = "aarch64") {
                    "arm64"
                } else {
                    "x86_64"
                };
                if arch != ours {
                    return false;
                }
            }
        }
        // Paprika turns on no optional features (demo mode, custom
        // resolution, quick play…), so a rule only matches when it asks for
        // them all to be off.
        if let Some(features) = &self.features {
            if features.values().any(|v| *v) {
                return false;
            }
        }
        true
    }
}

pub fn rules_allow(rules: &[Rule]) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for r in rules {
        if r.matches() {
            allowed = r.action == "allow";
        }
    }
    allowed
}

/// `group:artifact:version[:classifier][@ext]` → relative maven path.
pub fn maven_path(name: &str) -> Option<String> {
    let (coord, ext) = match name.split_once('@') {
        Some((c, e)) => (c, e),
        None => (name, "jar"),
    };
    let parts: Vec<&str> = coord.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let (group, artifact, version) = (parts[0], parts[1], parts[2]);
    let classifier = parts.get(3).map(|c| format!("-{c}")).unwrap_or_default();
    Some(format!(
        "{}/{artifact}/{version}/{artifact}-{version}{classifier}.{ext}",
        group.replace('.', "/")
    ))
}

/// Identity of a library ignoring its version, for de-duplication.
pub fn library_key(name: &str) -> String {
    let coord = name.split('@').next().unwrap_or(name);
    let parts: Vec<&str> = coord.split(':').collect();
    match parts.len() {
        0..=2 => coord.to_string(),
        3 => format!("{}:{}", parts[0], parts[1]),
        _ => format!("{}:{}:{}", parts[0], parts[1], parts[3]),
    }
}

/// Get (and cache) a vanilla version's JSON.
pub fn vanilla_version(manifest: &Manifest, id: &str) -> Result<VersionJson> {
    let path = paths::versions().join(id).join(format!("{id}.json"));
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(v) = serde_json::from_str::<VersionJson>(&text) {
            return Ok(v);
        }
    }
    let entry = manifest
        .versions
        .iter()
        .find(|v| v.id == id)
        .with_context(|| format!("Minecraft {id} isn't in Mojang's version list"))?;
    let text = net::get_string(&entry.url)?;
    let v: VersionJson = serde_json::from_str(&text).context("parsing version JSON")?;
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, text)?;
    Ok(v)
}

/// Put a mod loader profile on top of its vanilla parent.
pub fn merge(parent: VersionJson, child: VersionJson) -> VersionJson {
    let mut out = parent;
    out.id = child.id;
    if !child.main_class.is_empty() {
        out.main_class = child.main_class;
    }
    // Loader libraries win over Mojang ones with the same identity (ASM etc).
    let child_keys: std::collections::HashSet<String> = child
        .libraries
        .iter()
        .map(|l| library_key(&l.name))
        .collect();
    let mut libs = child.libraries;
    libs.extend(
        out.libraries
            .into_iter()
            .filter(|l| !child_keys.contains(&library_key(&l.name))),
    );
    out.libraries = libs;
    if let Some(args) = child.arguments {
        let mut base = out.arguments.take().unwrap_or_default();
        base.game.extend(args.game);
        base.jvm.extend(args.jvm);
        out.arguments = Some(base);
    }
    if child.minecraft_arguments.is_some() {
        out.minecraft_arguments = child.minecraft_arguments;
    }
    out
}

pub struct LibraryPlan {
    pub downloads: Vec<Download>,
    pub classpath: Vec<PathBuf>,
    pub natives: Vec<PathBuf>,
}

/// Work out which library files this OS needs.
pub fn plan_libraries(v: &VersionJson) -> LibraryPlan {
    let libdir = paths::libraries();
    let mut plan = LibraryPlan {
        downloads: vec![],
        classpath: vec![],
        natives: vec![],
    };
    let mut seen = std::collections::HashSet::new();
    for lib in &v.libraries {
        if !rules_allow(&lib.rules) {
            continue;
        }
        // Old-style natives: a classifier jar that gets extracted.
        if let Some(natives) = &lib.natives {
            if let Some(classifier) = natives.get(os_name()) {
                let arch = if cfg!(target_pointer_width = "64") {
                    "64"
                } else {
                    "32"
                };
                let classifier = classifier.replace("${arch}", arch);
                if let Some(art) = lib
                    .downloads
                    .as_ref()
                    .and_then(|d| d.classifiers.as_ref())
                    .and_then(|c| c.get(&classifier))
                {
                    let rel = art
                        .path
                        .clone()
                        .or_else(|| maven_path(&format!("{}:{classifier}", lib.name)));
                    if let Some(rel) = rel {
                        let p = libdir.join(&rel);
                        plan.downloads.push(
                            Download::new(&art.url, &p)
                                .sha1(art.sha1.clone())
                                .size(art.size),
                        );
                        plan.natives.push(p);
                    }
                }
            }
            if lib
                .downloads
                .as_ref()
                .and_then(|d| d.artifact.as_ref())
                .is_none()
            {
                continue;
            }
        }
        let (rel, url, sha1, size) = match lib.downloads.as_ref().and_then(|d| d.artifact.as_ref())
        {
            Some(art) => {
                let rel = match art.path.clone().or_else(|| maven_path(&lib.name)) {
                    Some(r) => r,
                    None => continue,
                };
                (rel, art.url.clone(), art.sha1.clone(), art.size)
            }
            None => {
                let Some(rel) = maven_path(&lib.name) else {
                    continue;
                };
                let mut base = lib.url.clone().unwrap_or_else(|| LIBRARIES_URL.to_string());
                if !base.ends_with('/') {
                    base.push('/');
                }
                (
                    rel.clone(),
                    format!("{base}{rel}"),
                    lib.sha1.clone(),
                    lib.size,
                )
            }
        };
        let p = libdir.join(&rel);
        if !seen.insert(p.clone()) {
            continue;
        }
        if !url.is_empty() {
            plan.downloads
                .push(Download::new(url, &p).sha1(sha1).size(size));
        }
        plan.classpath.push(p);
    }
    plan
}

#[derive(Deserialize)]
struct AssetIndex {
    objects: HashMap<String, AssetObject>,
    #[serde(default, rename = "virtual")]
    is_virtual: bool,
    #[serde(default)]
    map_to_resources: bool,
}

#[derive(Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

pub struct AssetPlan {
    pub index_id: String,
    pub downloads: Vec<Download>,
    /// For very old versions: (object file, name) pairs to copy into a
    /// "virtual" folder after downloading.
    pub legacy_copies: Vec<(PathBuf, PathBuf)>,
    pub game_assets: PathBuf,
}

pub fn plan_assets(v: &VersionJson) -> Result<AssetPlan> {
    let idx = v
        .asset_index
        .as_ref()
        .context("version has no asset index")?;
    let index_path = paths::assets()
        .join("indexes")
        .join(format!("{}.json", idx.id));
    net::download(&Download::new(&idx.url, &index_path).sha1(idx.sha1.clone()))?;
    let index: AssetIndex = serde_json::from_str(&std::fs::read_to_string(&index_path)?)?;
    let objects = paths::assets().join("objects");
    let mut plan = AssetPlan {
        index_id: idx.id.clone(),
        downloads: vec![],
        legacy_copies: vec![],
        game_assets: paths::assets().join("virtual").join(&idx.id),
    };
    for (name, obj) in &index.objects {
        let prefix = &obj.hash[..2.min(obj.hash.len())];
        let p = objects.join(prefix).join(&obj.hash);
        plan.downloads.push(
            Download::new(format!("{RESOURCES_URL}{prefix}/{}", obj.hash), &p)
                .sha1(Some(obj.hash.clone()))
                .size(Some(obj.size)),
        );
        if index.is_virtual || index.map_to_resources {
            plan.legacy_copies.push((p, plan.game_assets.join(name)));
        }
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maven_paths() {
        assert_eq!(
            maven_path("net.fabricmc:fabric-loader:0.16.9").unwrap(),
            "net/fabricmc/fabric-loader/0.16.9/fabric-loader-0.16.9.jar"
        );
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.3.3:natives-linux").unwrap(),
            "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-linux.jar"
        );
        assert_eq!(maven_path("a.b:c:1@zip").unwrap(), "a/b/c/1/c-1.zip");
        assert_eq!(library_key("org.ow2.asm:asm:9.6"), "org.ow2.asm:asm");
        assert_eq!(
            library_key("org.lwjgl:lwjgl:3.3.3:natives-linux"),
            "org.lwjgl:lwjgl:natives-linux"
        );
    }

    #[test]
    fn rules() {
        let other = if os_name() == "windows" {
            "linux"
        } else {
            "windows"
        };
        let r: Vec<Rule> = serde_json::from_str(&format!(
            r#"[{{"action":"allow"}},{{"action":"disallow","os":{{"name":"{other}"}}}}]"#
        ))
        .unwrap();
        assert!(rules_allow(&r));
        let r: Vec<Rule> = serde_json::from_str(&format!(
            r#"[{{"action":"allow","os":{{"name":"{other}"}}}}]"#
        ))
        .unwrap();
        assert!(!rules_allow(&r));
        let r: Vec<Rule> =
            serde_json::from_str(r#"[{"action":"allow","features":{"is_demo_user":true}}]"#)
                .unwrap();
        assert!(!rules_allow(&r));
    }

    #[test]
    fn parses_modern_and_fabric_json() {
        let v: VersionJson = serde_json::from_str(r#"{"id":"1.21.1","mainClass":"net.minecraft.client.main.Main","type":"release",
          "arguments":{"game":["--username","${auth_player_name}",{"rules":[{"action":"allow","features":{"has_custom_resolution":true}}],"value":["--width","${resolution_width}"]}],
          "jvm":[{"rules":[{"action":"allow","os":{"name":"osx"}}],"value":"-XstartOnFirstThread"},"-cp","${classpath}"]},
          "libraries":[{"name":"org.ow2.asm:asm:9.3","downloads":{"artifact":{"path":"org/ow2/asm/asm/9.3/asm-9.3.jar","url":"https://x/asm.jar","sha1":"aa","size":1}}}],
          "javaVersion":{"component":"java-runtime-delta","majorVersion":21}}"#).unwrap();
        let f: VersionJson = serde_json::from_str(r#"{"id":"fabric-loader-0.16.9-1.21.1","inheritsFrom":"1.21.1","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient",
          "arguments":{"game":[],"jvm":["-DFabricMcEmu= net.minecraft.client.main.Main "]},
          "libraries":[{"name":"org.ow2.asm:asm:9.7.1","url":"https://maven.fabricmc.net/","sha1":"bb","size":2}]}"#).unwrap();
        let m = merge(v, f);
        assert_eq!(
            m.main_class,
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
        assert_eq!(m.libraries.len(), 1);
        let plan = plan_libraries(&m);
        assert_eq!(
            plan.downloads[0].url,
            "https://maven.fabricmc.net/org/ow2/asm/asm/9.7.1/asm-9.7.1.jar"
        );
        assert_eq!(m.arguments.unwrap().jvm.len(), 4);
    }
}
