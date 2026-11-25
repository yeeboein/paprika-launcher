//! Instances: separate Minecraft setups, each with its own mods and saves.

use crate::loaders::Loader;
use crate::paths;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static LOCK: Mutex<()> = Mutex::new(());

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InstalledContent {
    pub project_id: String,
    pub version_id: String,
    pub title: String,
    pub filename: String,
    pub version_number: String,
    /// Folder inside the game dir: mods, resourcepacks or shaderpacks.
    pub folder: String,
    #[serde(default)]
    pub icon_url: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Instance {
    #[serde(skip)]
    pub id: String,
    pub name: String,
    pub mc_version: String,
    #[serde(default)]
    pub loader: Loader,
    #[serde(default)]
    pub loader_version: Option<String>,
    #[serde(default)]
    pub memory_mb: Option<u32>,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub content: Vec<InstalledContent>,
    #[serde(default)]
    pub last_played: u64,
    #[serde(default)]
    pub created: u64,
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn slug(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "instance".into()
    } else {
        s
    }
}

impl Instance {
    pub fn dir(&self) -> PathBuf {
        paths::instances().join(&self.id)
    }
    pub fn game_dir(&self) -> PathBuf {
        self.dir().join("minecraft")
    }
    pub fn label(&self) -> String {
        match self.loader {
            Loader::Vanilla => self.mc_version.clone(),
            l => format!("{} {}", l.label(), self.mc_version),
        }
    }

    pub fn create(name: &str, mc_version: &str, loader: Loader) -> Result<Instance> {
        let _g = LOCK.lock().unwrap();
        let base = slug(name);
        let mut id = base.clone();
        let mut n = 2;
        while paths::instances().join(&id).exists() {
            id = format!("{base}-{n}");
            n += 1;
        }
        let inst = Instance {
            id,
            name: name.trim().to_string(),
            mc_version: mc_version.to_string(),
            loader,
            loader_version: None,
            memory_mb: None,
            icon_url: None,
            content: vec![],
            last_played: 0,
            created: now(),
        };
        std::fs::create_dir_all(inst.game_dir())?;
        inst.write()?;
        Ok(inst)
    }

    fn write(&self) -> Result<()> {
        std::fs::create_dir_all(self.dir())?;
        let tmp = self.dir().join("instance.json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(tmp, self.dir().join("instance.json"))?;
        Ok(())
    }

    pub fn load(id: &str) -> Result<Instance> {
        let path = paths::instances().join(id).join("instance.json");
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let mut inst: Instance = serde_json::from_str(&text)?;
        inst.id = id.to_string();
        Ok(inst)
    }

    pub fn load_all() -> Vec<Instance> {
        let mut out = vec![];
        if let Ok(rd) = std::fs::read_dir(paths::instances()) {
            for e in rd.flatten() {
                if let Some(id) = e.file_name().to_str() {
                    if let Ok(i) = Instance::load(id) {
                        out.push(i);
                    }
                }
            }
        }
        out.sort_by(|a, b| {
            b.last_played
                .cmp(&a.last_played)
                .then(b.created.cmp(&a.created))
        });
        out
    }

    /// Load, change and save an instance without racing other threads.
    pub fn modify<R>(id: &str, f: impl FnOnce(&mut Instance) -> R) -> Result<R> {
        let _g = LOCK.lock().unwrap();
        let mut inst = Instance::load(id)?;
        let r = f(&mut inst);
        inst.write()?;
        crate::tasks::mark_instances_dirty();
        Ok(r)
    }

    pub fn delete(id: &str) -> Result<()> {
        if id.is_empty() || id.contains("..") || id.contains('/') || id.contains('\\') {
            bail!("bad instance id");
        }
        let _g = LOCK.lock().unwrap();
        std::fs::remove_dir_all(paths::instances().join(id))?;
        Ok(())
    }
}

/// A file in one of the content folders, plus its Modrinth info if known.
#[derive(Clone, Debug)]
pub struct ContentFile {
    pub folder: &'static str,
    pub filename: String,
    pub path: PathBuf,
    pub enabled: bool,
    pub meta: Option<InstalledContent>,
}

impl ContentFile {
    pub fn display_name(&self) -> String {
        match &self.meta {
            Some(m) => m.title.clone(),
            None => self.filename.trim_end_matches(".disabled").to_string(),
        }
    }
}

pub const CONTENT_FOLDERS: [&str; 3] = ["mods", "resourcepacks", "shaderpacks"];

pub fn list_content(inst: &Instance) -> Vec<ContentFile> {
    let mut out = vec![];
    for folder in CONTENT_FOLDERS {
        let Ok(rd) = std::fs::read_dir(inst.game_dir().join(folder)) else {
            continue;
        };
        for e in rd.flatten() {
            let Some(filename) = e.file_name().to_str().map(String::from) else {
                continue;
            };
            let path = e.path();
            let lower = filename.to_lowercase();
            let wanted = lower.ends_with(".jar")
                || lower.ends_with(".zip")
                || lower.ends_with(".disabled")
                || path.is_dir() && folder != "mods";
            if !wanted || filename.ends_with(".paprika-part") {
                continue;
            }
            let base = filename.trim_end_matches(".disabled");
            let meta = inst
                .content
                .iter()
                .find(|c| c.folder == folder && c.filename == base)
                .cloned();
            out.push(ContentFile {
                folder,
                enabled: !filename.ends_with(".disabled"),
                filename,
                path,
                meta,
            });
        }
    }
    out.sort_by_key(|c| (c.folder, c.display_name().to_lowercase()));
    out
}

pub fn toggle_content(inst_id: &str, file: &ContentFile) -> Result<()> {
    let new_path = if file.enabled {
        PathBuf::from(format!("{}.disabled", file.path.display()))
    } else {
        file.path
            .with_file_name(file.filename.trim_end_matches(".disabled"))
    };
    std::fs::rename(&file.path, new_path)?;
    crate::tasks::mark_instances_dirty();
    let _ = inst_id;
    Ok(())
}

pub fn remove_content(inst_id: &str, file: &ContentFile) -> Result<()> {
    if file.path.is_dir() {
        std::fs::remove_dir_all(&file.path)?;
    } else {
        std::fs::remove_file(&file.path)?;
    }
    let base = file.filename.trim_end_matches(".disabled").to_string();
    let folder = file.folder;
    Instance::modify(inst_id, |i| {
        i.content
            .retain(|c| !(c.folder == folder && c.filename == base))
    })?;
    Ok(())
}
