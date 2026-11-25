//! Modrinth: search, one-click installs with dependencies, updates and
//! `.mrpack` modpacks.

use crate::instance::{InstalledContent, Instance};
use crate::loaders::Loader;
use crate::net::{self, describe, Download};
use crate::tasks::TaskRef;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

const API: &str = "https://api.modrinth.com/v2";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProjectType {
    Mod,
    Modpack,
    ResourcePack,
    Shader,
}

impl ProjectType {
    pub const ALL: [ProjectType; 4] = [
        ProjectType::Mod,
        ProjectType::Modpack,
        ProjectType::ResourcePack,
        ProjectType::Shader,
    ];
    pub fn id(self) -> &'static str {
        match self {
            ProjectType::Mod => "mod",
            ProjectType::Modpack => "modpack",
            ProjectType::ResourcePack => "resourcepack",
            ProjectType::Shader => "shader",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            ProjectType::Mod => "Mods",
            ProjectType::Modpack => "Modpacks",
            ProjectType::ResourcePack => "Resource Packs",
            ProjectType::Shader => "Shaders",
        }
    }
    pub fn folder(self) -> &'static str {
        match self {
            ProjectType::Mod | ProjectType::Modpack => "mods",
            ProjectType::ResourcePack => "resourcepacks",
            ProjectType::Shader => "shaderpacks",
        }
    }
    pub fn from_id(s: &str) -> ProjectType {
        match s {
            "modpack" => ProjectType::Modpack,
            "resourcepack" => ProjectType::ResourcePack,
            "shader" => ProjectType::Shader,
            _ => ProjectType::Mod,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sort {
    Relevance,
    Downloads,
    Follows,
    Newest,
    Updated,
}

impl Sort {
    pub const ALL: [Sort; 5] = [
        Sort::Relevance,
        Sort::Downloads,
        Sort::Follows,
        Sort::Newest,
        Sort::Updated,
    ];
    pub fn id(self) -> &'static str {
        match self {
            Sort::Relevance => "relevance",
            Sort::Downloads => "downloads",
            Sort::Follows => "follows",
            Sort::Newest => "newest",
            Sort::Updated => "updated",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Sort::Relevance => "Relevance",
            Sort::Downloads => "Downloads",
            Sort::Follows => "Followers",
            Sort::Newest => "Newest",
            Sort::Updated => "Recently updated",
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct SearchHit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub follows: u64,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub categories: Vec<String>,
    pub project_type: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    pub total_hits: u64,
}

pub const PAGE_SIZE: u64 = 20;

pub fn search(
    query: &str,
    ptype: ProjectType,
    game_version: Option<&str>,
    loader: Option<Loader>,
    sort: Sort,
    offset: u64,
) -> Result<SearchResults> {
    let mut facets: Vec<Vec<String>> = vec![vec![format!("project_type:{}", ptype.id())]];
    if let Some(v) = game_version {
        facets.push(vec![format!("versions:{v}")]);
    }
    if matches!(ptype, ProjectType::Mod | ProjectType::Modpack) {
        if let Some(l) = loader.filter(|l| *l != Loader::Vanilla) {
            facets.push(
                l.modrinth_ids()
                    .into_iter()
                    .map(|id| format!("categories:{id}"))
                    .collect(),
            );
        }
    }
    let resp = net::agent()
        .get(&format!("{API}/search"))
        .query("query", query)
        .query("facets", &serde_json::to_string(&facets)?)
        .query("index", sort.id())
        .query("offset", &offset.to_string())
        .query("limit", &PAGE_SIZE.to_string())
        .call()
        .map_err(describe)?;
    Ok(resp.into_json()?)
}

#[derive(Deserialize, Clone, Debug)]
pub struct Version {
    pub id: String,
    pub project_id: String,
    pub version_number: String,
    #[serde(default)]
    pub version_type: String,
    #[serde(default)]
    pub loaders: Vec<String>,
    pub files: Vec<VersionFile>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct VersionFile {
    pub url: String,
    pub filename: String,
    #[serde(default)]
    pub primary: bool,
    #[serde(default)]
    pub hashes: HashMap<String, String>,
    #[serde(default)]
    pub size: u64,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Dependency {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub version_id: Option<String>,
    pub dependency_type: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Project {
    pub title: String,
    #[serde(default)]
    pub icon_url: Option<String>,
}

impl Version {
    pub fn primary_file(&self) -> Option<&VersionFile> {
        self.files.iter().find(|f| f.primary).or(self.files.first())
    }
}

pub fn project(id: &str) -> Result<Project> {
    net::get_json(&format!("{API}/project/{id}"))
}

pub fn version(id: &str) -> Result<Version> {
    net::get_json(&format!("{API}/version/{id}"))
}

pub fn project_versions(
    id: &str,
    game_version: Option<&str>,
    loaders: &[String],
) -> Result<Vec<Version>> {
    let mut req = net::agent().get(&format!("{API}/project/{id}/version"));
    if let Some(v) = game_version {
        req = req.query("game_versions", &serde_json::to_string(&[v])?);
    }
    if !loaders.is_empty() {
        req = req.query("loaders", &serde_json::to_string(loaders)?);
    }
    Ok(req.call().map_err(describe)?.into_json()?)
}

fn loaders_for(inst: &Instance, ptype: ProjectType) -> Vec<String> {
    match ptype {
        ProjectType::Mod => inst.loader.modrinth_ids(),
        _ => vec![],
    }
}

/// Newest compatible version, preferring full releases over betas.
fn best_version(project_id: &str, inst: &Instance, ptype: ProjectType) -> Result<Version> {
    let versions = project_versions(
        project_id,
        Some(&inst.mc_version),
        &loaders_for(inst, ptype),
    )?;
    versions
        .iter()
        .find(|v| v.version_type == "release")
        .or(versions.first())
        .cloned()
        .with_context(|| format!("no version works with {}", inst.label()))
}

/// Install a project (and its required dependencies) into an instance.
pub fn install(
    inst_id: &str,
    project_id: &str,
    title: &str,
    icon: Option<String>,
    ptype: ProjectType,
    task: &TaskRef,
) -> Result<()> {
    let inst = Instance::load(inst_id)?;
    if ptype == ProjectType::Mod && inst.loader == Loader::Vanilla {
        bail!(
            "{} is a vanilla instance - mods need Fabric or Quilt",
            inst.name
        );
    }
    let mut visited = HashSet::new();
    task.status(format!("Finding a version of {title}"));
    let v = best_version(project_id, &inst, ptype)?;
    install_version(&inst, v, title.to_string(), icon, ptype, task, &mut visited)
}

fn install_version(
    inst: &Instance,
    v: Version,
    title: String,
    icon: Option<String>,
    ptype: ProjectType,
    task: &TaskRef,
    visited: &mut HashSet<String>,
) -> Result<()> {
    if !visited.insert(v.project_id.clone()) {
        return Ok(());
    }
    let file = v.primary_file().context("version has no files")?.clone();
    let folder = ptype.folder();
    let dir = inst.game_dir().join(folder);
    let filename = safe_filename(&file.filename)?;
    task.status(format!("Downloading {title} {}", v.version_number));
    net::download(
        &Download::new(&file.url, dir.join(&filename))
            .sha1(file.hashes.get("sha1").cloned())
            .size(Some(file.size).filter(|s| *s > 0)),
    )?;

    // Replace any older copy of the same project.
    let old: Vec<String> = inst
        .content
        .iter()
        .filter(|c| c.project_id == v.project_id && c.filename != filename)
        .map(|c| c.filename.clone())
        .collect();
    for f in &old {
        let _ = std::fs::remove_file(dir.join(f));
        let _ = std::fs::remove_file(dir.join(format!("{f}.disabled")));
    }
    let entry = InstalledContent {
        project_id: v.project_id.clone(),
        version_id: v.id.clone(),
        title: title.clone(),
        filename: filename.clone(),
        version_number: v.version_number.clone(),
        folder: folder.to_string(),
        icon_url: icon,
    };
    Instance::modify(&inst.id, |i| {
        i.content.retain(|c| c.project_id != entry.project_id);
        i.content.push(entry);
    })?;
    crate::tasks::log(format!(
        "[paprika] Installed {title} {} into {}",
        v.version_number, inst.name
    ));

    if ptype != ProjectType::Mod {
        return Ok(());
    }
    for dep in v
        .dependencies
        .iter()
        .filter(|d| d.dependency_type == "required")
    {
        let current = Instance::load(&inst.id)?;
        let dep_version = match (&dep.version_id, &dep.project_id) {
            (Some(vid), _) => version(vid)?,
            (None, Some(pid)) => {
                if current.content.iter().any(|c| &c.project_id == pid) {
                    continue;
                }
                best_version(pid, &current, ProjectType::Mod)?
            }
            _ => continue,
        };
        if current
            .content
            .iter()
            .any(|c| c.project_id == dep_version.project_id)
            || visited.contains(&dep_version.project_id)
        {
            continue;
        }
        let p = project(&dep_version.project_id)?;
        install_version(
            &current,
            dep_version,
            p.title,
            p.icon_url,
            ProjectType::Mod,
            task,
            visited,
        )?;
    }
    Ok(())
}

fn safe_filename(name: &str) -> Result<String> {
    let n = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .context("bad file name")?;
    if n.is_empty() || n == "." || n == ".." {
        bail!("bad file name");
    }
    Ok(n.to_string())
}

/// Update every mod Modrinth knows about to its newest compatible version.
pub fn update_all(inst_id: &str, task: &TaskRef) -> Result<usize> {
    let inst = Instance::load(inst_id)?;
    let dir = inst.game_dir().join("mods");
    task.status("Hashing installed mods");
    let mut by_hash: HashMap<String, PathBuf> = HashMap::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("jar") {
                if let Ok(h) = net::file_sha1(&p) {
                    by_hash.insert(h, p);
                }
            }
        }
    }
    if by_hash.is_empty() {
        return Ok(0);
    }
    task.status("Checking Modrinth for updates");
    let body = json!({
        "hashes": by_hash.keys().collect::<Vec<_>>(),
        "algorithm": "sha1",
        "loaders": inst.loader.modrinth_ids(),
        "game_versions": [inst.mc_version],
    });
    let resp = net::agent()
        .post(&format!("{API}/version_files/update"))
        .send_json(body)
        .map_err(describe)?;
    let updates: HashMap<String, Version> = resp.into_json()?;
    let mut count = 0;
    let todo: Vec<(PathBuf, Version)> = updates
        .into_iter()
        .filter_map(|(hash, v)| {
            let path = by_hash.get(&hash)?.clone();
            let f = v.primary_file()?;
            (f.hashes.get("sha1") != Some(&hash)).then_some((path, v))
        })
        .collect();
    task.set_progress(0, todo.len());
    for (old_path, v) in todo {
        let file = v.primary_file().unwrap().clone();
        let filename = safe_filename(&file.filename)?;
        task.status(format!("Updating {filename}"));
        net::download(
            &Download::new(&file.url, dir.join(&filename)).sha1(file.hashes.get("sha1").cloned()),
        )?;
        if old_path != dir.join(&filename) {
            let _ = std::fs::remove_file(&old_path);
        }
        let old_name = old_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        let (pid, vid, num) = (v.project_id.clone(), v.id.clone(), v.version_number.clone());
        let fname = filename.clone();
        Instance::modify(inst_id, move |i| {
            if let Some(c) = i
                .content
                .iter_mut()
                .find(|c| c.project_id == pid || c.filename == old_name)
            {
                c.filename = fname;
                c.version_id = vid;
                c.version_number = num;
            }
        })?;
        count += 1;
        task.inc();
    }
    crate::tasks::log(format!("[paprika] Updated {count} mod(s) in {}", inst.name));
    Ok(count)
}

/// Is a relative path from a modpack safe to write inside the game folder?
fn safe_relative(p: &str) -> Option<PathBuf> {
    let path = Path::new(p);
    if path.components().all(|c| matches!(c, Component::Normal(_))) && !p.is_empty() {
        Some(path.to_path_buf())
    } else {
        None
    }
}

/// Download a Modrinth modpack and turn it into a new instance.
pub fn install_modpack(
    project_id: &str,
    title: &str,
    icon: Option<String>,
    task: &TaskRef,
) -> Result<String> {
    task.status(format!("Finding the newest {title}"));
    let versions = project_versions(project_id, None, &[])?;
    let v = versions
        .iter()
        .find(|v| {
            v.version_type == "release"
                && v.loaders
                    .iter()
                    .any(|l| l == "fabric" || l == "quilt" || l == "minecraft")
        })
        .or_else(|| {
            versions
                .iter()
                .find(|v| v.loaders.iter().any(|l| l == "fabric" || l == "quilt"))
        })
        .or(versions.first())
        .context("this modpack has no versions")?;
    let file = v.primary_file().context("no .mrpack file")?;
    let pack_path = crate::paths::cache()
        .join("packs")
        .join(format!("{}.mrpack", v.id));
    task.status("Downloading modpack");
    net::download(&Download::new(&file.url, &pack_path).sha1(file.hashes.get("sha1").cloned()))?;

    let mut zip = zip::ZipArchive::new(std::fs::File::open(&pack_path)?)?;
    let index: Value = {
        let mut s = String::new();
        zip.by_name("modrinth.index.json")
            .context("not a Modrinth pack")?
            .read_to_string(&mut s)?;
        serde_json::from_str(&s)?
    };
    let deps = &index["dependencies"];
    let mc = deps["minecraft"]
        .as_str()
        .context("pack doesn't say its Minecraft version")?;
    let (loader, loader_version) = if let Some(l) = deps["fabric-loader"].as_str() {
        (Loader::Fabric, Some(l.to_string()))
    } else if let Some(l) = deps["quilt-loader"].as_str() {
        (Loader::Quilt, Some(l.to_string()))
    } else if deps.get("forge").is_some() || deps.get("neoforge").is_some() {
        bail!("{title} needs Forge/NeoForge, which Paprika doesn't support yet - try a Fabric or Quilt pack");
    } else {
        (Loader::Vanilla, None)
    };

    let inst = Instance::create(title, mc, loader)?;
    let id = inst.id.clone();
    Instance::modify(&id, |i| {
        i.loader_version = loader_version;
        i.icon_url = icon;
    })?;
    let game = inst.game_dir();
    let result = (|| -> Result<()> {
        let mut downloads = vec![];
        for f in index["files"].as_array().into_iter().flatten() {
            if f["env"]["client"].as_str() == Some("unsupported") {
                continue;
            }
            let Some(rel) = f["path"].as_str().and_then(safe_relative) else {
                continue;
            };
            let Some(url) = f["downloads"][0].as_str() else {
                continue;
            };
            downloads.push(
                Download::new(url, game.join(rel))
                    .sha1(f["hashes"]["sha1"].as_str().map(String::from))
                    .size(f["fileSize"].as_u64()),
            );
        }
        task.status(format!("Downloading {} pack files", downloads.len()));
        net::download_all(downloads, task)?;
        task.status("Copying pack config");
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            let Some(name) = entry.enclosed_name() else {
                continue;
            };
            let rel = if let Ok(r) = name.strip_prefix("overrides") {
                r.to_path_buf()
            } else if let Ok(r) = name.strip_prefix("client-overrides") {
                r.to_path_buf()
            } else {
                continue;
            };
            if rel.as_os_str().is_empty() {
                continue;
            }
            let out = game.join(rel);
            if entry.is_dir() {
                std::fs::create_dir_all(&out)?;
            } else {
                if let Some(p) = out.parent() {
                    std::fs::create_dir_all(p)?;
                }
                std::io::copy(&mut entry, &mut std::fs::File::create(&out)?)?;
            }
        }
        Ok(())
    })();
    if let Err(e) = result {
        let _ = Instance::delete(&id);
        return Err(e);
    }
    crate::tasks::log(format!(
        "[paprika] Installed modpack {title} ({}, {})",
        v.version_number,
        inst.label()
    ));
    Ok(id)
}
