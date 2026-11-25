// Paprika - a lightweight Minecraft launcher by Yeeboein.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod auth;
mod config;
mod images;
mod instance;
mod java;
mod launch;
mod loaders;
mod modrinth;
mod mojang;
mod net;
mod paths;
mod tasks;
mod theme;

fn main() -> eframe::Result<()> {
    let _ = std::fs::create_dir_all(paths::instances());
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Paprika")
            .with_app_id("paprika")
            .with_inner_size([1120.0, 720.0])
            .with_min_inner_size([820.0, 520.0])
            .with_icon(theme::window_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "Paprika",
        options,
        Box::new(|cc| Ok(Box::new(app::PaprikaApp::new(cc)))),
    )
}

#[cfg(test)]
mod e2e {
    //! Network test: `cargo test -- --ignored --nocapture` (needs internet).
    use crate::{
        auth, config, instance::Instance, launch, loaders::Loader, modrinth, tasks::TaskManager,
    };
    use std::time::Duration;

    fn run<F: FnOnce(&crate::tasks::TaskRef) -> anyhow::Result<()> + Send + 'static>(f: F) {
        let tm = TaskManager::default();
        let t = tm.spawn("test", f);
        while t.finished_at().is_none() {
            std::thread::sleep(Duration::from_millis(200));
        }
        if let Some(e) = t.error() {
            panic!("{e}");
        }
    }

    #[test]
    #[ignore]
    fn install_and_launch_fabric() {
        let inst = Instance::create("E2E Fabric", "1.21.1", Loader::Fabric).unwrap();
        let id = inst.id.clone();
        let i2 = id.clone();
        // Iris requires Sodium, so this also checks dependency installs.
        run(move |t| {
            modrinth::install(
                &i2,
                "YL57xq9U",
                "Iris Shaders",
                None,
                modrinth::ProjectType::Mod,
                t,
            )
        });
        let inst = Instance::load(&id).unwrap();
        let names: Vec<_> = inst.content.iter().map(|c| c.title.clone()).collect();
        println!("installed: {names:?}");
        assert!(names.len() >= 2, "dependency missing: {names:?}");
        let results = modrinth::search(
            "sodium",
            modrinth::ProjectType::Mod,
            Some("1.21.1"),
            Some(Loader::Fabric),
            modrinth::Sort::Relevance,
            0,
        )
        .unwrap();
        assert!(!results.hits.is_empty());
        let running = launch::Running::default();
        let r2 = running.clone();
        let i3 = id.clone();
        run(move |t| {
            launch::launch(
                &i3,
                &auth::offline("PaprikaTest"),
                &config::Settings::default(),
                r2,
                t,
            )
        });
        std::thread::sleep(Duration::from_secs(40));
        for l in crate::tasks::log_snapshot() {
            println!("{l}");
        }
        if let Some(c) = running.lock().unwrap().get_mut(&id) {
            let _ = c.kill();
        };
    }

    #[test]
    #[ignore]
    fn install_modpack() {
        let tm = TaskManager::default();
        let t = tm.spawn("pack", |t| {
            let id = modrinth::install_modpack("1KVo5zza", "Fabulously Optimized", None, t)?;
            let inst = Instance::load(&id)?;
            let mods = std::fs::read_dir(inst.game_dir().join("mods"))?.count();
            println!(
                "pack instance {} ({}) has {mods} mods, loader {:?}",
                inst.name,
                inst.label(),
                inst.loader_version
            );
            anyhow::ensure!(mods > 5);
            Ok(())
        });
        while t.finished_at().is_none() {
            std::thread::sleep(Duration::from_millis(200));
        }
        assert!(t.error().is_none(), "{:?}", t.error());
    }
}
