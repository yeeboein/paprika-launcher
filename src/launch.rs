//! Prepare an instance (download everything) and start the game.

use crate::auth::{Account, AccountKind};
use crate::config::Settings;
use crate::instance::Instance;
use crate::loaders::{self, Loader};
use crate::mojang::{self, Arg, StrOrVec, VersionJson};
use crate::net;
use crate::tasks::{log, TaskRef};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

pub type Running = Arc<Mutex<HashMap<String, Child>>>;

const CP_SEP: &str = if cfg!(target_os = "windows") {
    ";"
} else {
    ":"
};

fn extract_natives(jars: &[PathBuf], dir: &Path) -> Result<()> {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir)?;
    for jar in jars {
        let mut zip = zip::ZipArchive::new(std::fs::File::open(jar)?)?;
        for i in 0..zip.len() {
            let mut e = zip.by_index(i)?;
            let Some(name) = e.enclosed_name() else {
                continue;
            };
            if e.is_dir() || name.starts_with("META-INF") {
                continue;
            }
            let out = dir.join(name);
            if let Some(p) = out.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::io::copy(&mut e, &mut std::fs::File::create(out)?)?;
        }
    }
    Ok(())
}

fn expand(args: &[Arg], out: &mut Vec<String>) {
    for a in args {
        match a {
            Arg::Plain(s) => out.push(s.clone()),
            Arg::Ruled { rules, value } => {
                if mojang::rules_allow(rules) {
                    match value {
                        StrOrVec::One(s) => out.push(s.clone()),
                        StrOrVec::Many(v) => out.extend(v.iter().cloned()),
                    }
                }
            }
        }
    }
}

fn substitute(s: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = s.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("${{{k}}}"), v);
    }
    out
}

/// Download whatever is missing and start Minecraft. Returns once the game
/// process is running.
pub fn launch(
    inst_id: &str,
    account: &Account,
    settings: &Settings,
    running: Running,
    task: &TaskRef,
) -> Result<()> {
    let mut inst = Instance::load(inst_id)?;
    task.status("Reading version info");
    let manifest = mojang::fetch_manifest()?;
    let vanilla = mojang::vanilla_version(&manifest, &inst.mc_version)?;

    // Java
    let java = if !settings.java_path.trim().is_empty() {
        PathBuf::from(settings.java_path.trim())
    } else {
        let component = vanilla
            .java_version
            .as_ref()
            .map(|j| j.component.clone())
            .unwrap_or_else(|| "jre-legacy".into());
        crate::java::ensure_runtime(&component, task)?
    };

    // Vanilla client jar (Forge's installer needs it too)
    task.status("Checking game files");
    let vanilla_jar = crate::paths::versions()
        .join(&inst.mc_version)
        .join(format!("{}.jar", inst.mc_version));
    if let Some(c) = vanilla.downloads.as_ref().and_then(|d| d.client.as_ref()) {
        net::download(
            &net::Download::new(&c.url, &vanilla_jar)
                .sha1(c.sha1.clone())
                .size(c.size),
        )?;
    }

    let version: VersionJson = if inst.loader == Loader::Vanilla {
        vanilla.clone()
    } else {
        let lv = match &inst.loader_version {
            Some(v) => v.clone(),
            None => {
                task.status(format!("Picking a {} version", inst.loader.label()));
                let v = loaders::latest_loader(inst.loader, &inst.mc_version)?;
                let vv = v.clone();
                Instance::modify(inst_id, |i| i.loader_version = Some(vv))?;
                inst.loader_version = Some(v.clone());
                v
            }
        };
        task.status(format!("Getting {} {lv}", inst.loader.label()));
        let profile = if inst.loader.is_forge_like() {
            crate::forge::install(
                inst.loader,
                &inst.mc_version,
                &lv,
                &vanilla_jar,
                &java,
                task,
            )?
        } else {
            loaders::profile(inst.loader, &inst.mc_version, &lv)?
        };
        mojang::merge(vanilla.clone(), profile)
    };

    // Forge excludes "<version id>.jar" from its module layer, so the game
    // jar must carry the loader profile's name, like the official launcher.
    let client_jar = if inst.loader.is_forge_like() {
        let named = crate::paths::versions()
            .join(&version.id)
            .join(format!("{}.jar", version.id));
        if !named.exists() {
            std::fs::create_dir_all(named.parent().unwrap())?;
            std::fs::copy(&vanilla_jar, &named)?;
        }
        named
    } else {
        vanilla_jar
    };
    let mut downloads = vec![];
    let libs = mojang::plan_libraries(&version);
    downloads.extend(libs.downloads.iter().cloned());
    task.status("Downloading libraries");
    net::download_all(downloads, task)?;

    // Assets
    task.status("Checking assets");
    let assets = mojang::plan_assets(&vanilla)?;
    task.status("Downloading assets");
    net::download_all(assets.downloads, task)?;
    for (from, to) in &assets.legacy_copies {
        if !to.exists() {
            if let Some(p) = to.parent() {
                std::fs::create_dir_all(p)?;
            }
            let _ = std::fs::copy(from, to);
        }
    }
    task.set_progress(0, 0);

    task.status("Unpacking natives");
    let natives_dir = inst.dir().join("natives");
    extract_natives(&libs.natives, &natives_dir)?;

    let game_dir = inst.game_dir();
    std::fs::create_dir_all(&game_dir)?;
    let mut cp: Vec<String> = libs
        .classpath
        .iter()
        .map(|p| p.display().to_string())
        .collect();
    cp.push(client_jar.display().to_string());

    let mut vars: HashMap<&str, String> = HashMap::new();
    vars.insert("auth_player_name", account.username.clone());
    vars.insert("version_name", version.id.clone());
    vars.insert("game_directory", game_dir.display().to_string());
    vars.insert("assets_root", crate::paths::assets().display().to_string());
    vars.insert("game_assets", assets.game_assets.display().to_string());
    vars.insert("assets_index_name", assets.index_id.clone());
    vars.insert("auth_uuid", account.uuid.clone());
    vars.insert("auth_access_token", account.access_token.clone());
    vars.insert("auth_session", account.access_token.clone());
    vars.insert("auth_xuid", "0".into());
    vars.insert("clientid", "0".into());
    vars.insert(
        "user_type",
        if account.kind == AccountKind::Microsoft {
            "msa".into()
        } else {
            "legacy".into()
        },
    );
    vars.insert("user_properties", "{}".into());
    vars.insert(
        "version_type",
        vanilla.kind.clone().unwrap_or_else(|| "release".into()),
    );
    vars.insert("natives_directory", natives_dir.display().to_string());
    vars.insert("launcher_name", "paprika".into());
    vars.insert("launcher_version", env!("CARGO_PKG_VERSION").into());
    vars.insert("classpath", cp.join(CP_SEP));
    vars.insert("classpath_separator", CP_SEP.into());
    vars.insert(
        "library_directory",
        crate::paths::libraries().display().to_string(),
    );

    let mem = inst.memory_mb.unwrap_or(settings.memory_mb).max(512);
    let mut args: Vec<String> = vec![format!("-Xmx{mem}M"), "-Xms512M".into()];
    args.extend(settings.jvm_args.split_whitespace().map(String::from));
    let mut jvm = vec![];
    let mut game = vec![];
    match &version.arguments {
        Some(a) if !a.jvm.is_empty() || !a.game.is_empty() => {
            expand(&a.jvm, &mut jvm);
            expand(&a.game, &mut game);
        }
        _ => {}
    }
    if jvm.is_empty() {
        jvm = vec![
            "-Djava.library.path=${natives_directory}".into(),
            "-cp".into(),
            "${classpath}".into(),
        ];
    }
    if let Some(legacy) = &version.minecraft_arguments {
        game = legacy.split_whitespace().map(String::from).collect();
    }
    args.extend(jvm.iter().map(|a| substitute(a, &vars)));
    args.push(version.main_class.clone());
    args.extend(game.iter().map(|a| substitute(a, &vars)));
    args.extend([
        "--width".into(),
        settings.width.to_string(),
        "--height".into(),
        settings.height.to_string(),
    ]);

    task.status("Starting Minecraft");
    log(format!(
        "[paprika] Launching {} ({}) as {}",
        inst.name, version.id, account.username
    ));
    log(format!("[paprika] Java: {}", java.display()));
    let mut cmd = Command::new(&java);
    cmd.args(&args)
        .current_dir(&game_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd
        .spawn()
        .with_context(|| format!("couldn't start Java at {}", java.display()))?;
    for stream in [
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
    ]
    .into_iter()
    .flatten()
    {
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                log(line);
            }
        });
    }
    running.lock().unwrap().insert(inst_id.to_string(), child);
    Instance::modify(inst_id, |i| i.last_played = crate::instance::now())?;

    // Watch for the game exiting.
    let id = inst_id.to_string();
    let name = inst.name.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let mut map = running.lock().unwrap();
        let Some(child) = map.get_mut(&id) else { break };
        match child.try_wait() {
            Ok(Some(status)) => {
                map.remove(&id);
                log(format!("[paprika] {name} exited ({status})"));
                crate::tasks::repaint();
                break;
            }
            Ok(None) => {}
            Err(_) => {
                map.remove(&id);
                break;
            }
        }
    });
    Ok(())
}
