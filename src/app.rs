//! The Paprika window.

use crate::auth::{self, Account, AccountKind, DeviceCode};
use crate::config::{self, Accounts, Settings};
use crate::instance::{self, Instance};
use crate::launch::Running;
use crate::loaders::Loader;
use crate::modrinth::{self, ProjectType, SearchHit, SearchResults, Sort};
use crate::mojang::Manifest;
use crate::tasks::{self, TaskManager, TaskRef};
use crate::theme::{self, ACCENT, ACCENT_HOVER, CARD, CARD_HOVER, GREEN, ORANGE, PANEL, TEXT_DIM};
use eframe::egui::{self, vec2, Align, Color32, Frame, Layout, RichText, Sense, Stroke, Ui};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Library,
    Browse,
    Accounts,
    Settings,
    Console,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InstTab {
    Content,
    Options,
}

enum Msg {
    Manifest(Result<Manifest, String>),
    Search(u64, Result<SearchResults, String>),
    DeviceCode(DeviceCode),
    Login(Result<Account, String>),
    AccountRefreshed(Account),
    OpenInstance(String),
}

struct NewInstance {
    name: String,
    version: String,
    loader: Loader,
    snapshots: bool,
}

struct Browse {
    query: String,
    ptype: ProjectType,
    sort: Sort,
    target: Option<String>,
    results: Vec<SearchHit>,
    total: u64,
    offset: u64,
    loading: bool,
    error: Option<String>,
    generation: u64,
    needs_search: bool,
}

struct MsLogin {
    code: Option<DeviceCode>,
    cancel: Arc<AtomicBool>,
    error: Option<String>,
}

pub struct PaprikaApp {
    page: Page,
    instances: Vec<Instance>,
    selected: Option<String>,
    inst_tab: InstTab,
    content_cache: Vec<instance::ContentFile>,
    settings: Settings,
    accounts: Accounts,
    manifest: Option<Manifest>,
    manifest_error: Option<String>,
    new_instance: Option<NewInstance>,
    browse: Browse,
    tasks: TaskManager,
    installing: HashMap<String, TaskRef>,
    running: Running,
    ms_login: Option<MsLogin>,
    offline_name: String,
    confirm_delete: Option<String>,
    rename_buf: String,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
}

impl PaprikaApp {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        tasks::set_ctx(cc.egui_ctx.clone());
        egui_extras::install_image_loaders(&cc.egui_ctx);
        cc.egui_ctx
            .add_bytes_loader(Arc::new(crate::images::HttpLoader::default()));
        theme::apply(&cc.egui_ctx);
        let (tx, rx) = channel();
        let t = tx.clone();
        std::thread::spawn(move || {
            let _ = t.send(Msg::Manifest(
                crate::mojang::fetch_manifest().map_err(|e| format!("{e:#}")),
            ));
            tasks::repaint();
        });
        tasks::log(format!(
            "[paprika] Paprika Launcher {} by Yeeboein - data folder: {}",
            env!("CARGO_PKG_VERSION"),
            crate::paths::root().display()
        ));
        let mut app = PaprikaApp {
            page: Page::Library,
            instances: Instance::load_all(),
            selected: None,
            inst_tab: InstTab::Content,
            content_cache: vec![],
            settings: config::load_settings(),
            accounts: config::load_accounts(),
            manifest: None,
            manifest_error: None,
            new_instance: None,
            browse: Browse {
                query: String::new(),
                ptype: ProjectType::Mod,
                sort: Sort::Relevance,
                target: None,
                results: vec![],
                total: 0,
                offset: 0,
                loading: false,
                error: None,
                generation: 0,
                needs_search: true,
            },
            tasks: TaskManager::default(),
            installing: HashMap::new(),
            running: Running::default(),
            ms_login: None,
            offline_name: String::new(),
            confirm_delete: None,
            rename_buf: String::new(),
            tx,
            rx,
        };
        app.browse.target = app
            .instances
            .iter()
            .find(|i| i.loader != Loader::Vanilla)
            .map(|i| i.id.clone());
        app
    }

    fn instance(&self, id: &str) -> Option<&Instance> {
        self.instances.iter().find(|i| i.id == id)
    }

    fn reload_instances(&mut self) {
        self.instances = Instance::load_all();
        if let Some(sel) = &self.selected {
            if self.instance(sel).is_none() {
                self.selected = None;
            }
        }
        if let Some(t) = &self.browse.target {
            if self.instance(t).is_none() {
                self.browse.target = None;
            }
        }
        self.refresh_content();
    }

    fn refresh_content(&mut self) {
        self.content_cache = match self.selected.as_ref().and_then(|s| self.instance(s)) {
            Some(i) => instance::list_content(i),
            None => vec![],
        };
    }

    fn handle_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Manifest(Ok(m)) => {
                    self.manifest = Some(m);
                    self.manifest_error = None;
                }
                Msg::Manifest(Err(e)) => self.manifest_error = Some(e),
                Msg::Search(gen, res) if gen == self.browse.generation => {
                    self.browse.loading = false;
                    match res {
                        Ok(r) => {
                            self.browse.total = r.total_hits;
                            self.browse.results = r.hits;
                            self.browse.error = None;
                        }
                        Err(e) => self.browse.error = Some(e),
                    }
                }
                Msg::Search(..) => {}
                Msg::DeviceCode(dc) => {
                    if let Some(l) = &mut self.ms_login {
                        let _ = open::that(&dc.verification_uri);
                        l.code = Some(dc);
                    }
                }
                Msg::Login(Ok(acc)) => {
                    tasks::log(format!("[paprika] Signed in as {}", acc.username));
                    self.accounts.upsert(acc);
                    config::save_accounts(&self.accounts);
                    self.ms_login = None;
                }
                Msg::Login(Err(e)) => {
                    if let Some(l) = &mut self.ms_login {
                        l.error = Some(e);
                        l.code = None;
                    }
                }
                Msg::AccountRefreshed(acc) => {
                    let active = self.accounts.active.clone();
                    self.accounts.upsert(acc);
                    self.accounts.active = active;
                    config::save_accounts(&self.accounts);
                }
                Msg::OpenInstance(id) => {
                    self.reload_instances();
                    self.selected = Some(id);
                    self.page = Page::Library;
                    self.inst_tab = InstTab::Content;
                    self.refresh_content();
                }
            }
        }
    }

    // ---------- actions ----------

    fn play(&mut self, id: &str) {
        let Some(acc) = self.accounts.active().cloned() else {
            self.page = Page::Accounts;
            return;
        };
        let settings = self.settings.clone();
        let running = self.running.clone();
        let tx = self.tx.clone();
        let id = id.to_string();
        let name = self
            .instance(&id)
            .map(|i| i.name.clone())
            .unwrap_or_default();
        self.tasks.spawn(format!("Launching {name}"), move |task| {
            task.status("Checking account");
            let fresh = auth::ensure_fresh(&acc, settings.client_id().as_deref())?;
            if fresh.access_token != acc.access_token {
                let _ = tx.send(Msg::AccountRefreshed(fresh.clone()));
            }
            crate::launch::launch(&id, &fresh, &settings, running, task)?;
            Ok(())
        });
    }

    fn stop(&mut self, id: &str) {
        if let Some(child) = self.running.lock().unwrap().get_mut(id) {
            let _ = child.kill();
        }
    }

    fn is_running(&self, id: &str) -> bool {
        self.running.lock().unwrap().contains_key(id)
    }

    fn is_launching(&self, id: &str) -> bool {
        let name = self.instance(id).map(|i| format!("Launching {}", i.name));
        self.tasks
            .list()
            .iter()
            .any(|t| Some(&t.title) == name.as_ref() && t.finished_at().is_none())
    }

    fn start_search(&mut self, ctx: &egui::Context) {
        self.browse.generation += 1;
        self.browse.loading = true;
        self.browse.needs_search = false;
        let gen = self.browse.generation;
        let target = self
            .browse
            .target
            .as_ref()
            .and_then(|t| self.instance(t))
            .cloned();
        let filter = self.browse.ptype != ProjectType::Modpack;
        let (query, ptype, sort, offset) = (
            self.browse.query.clone(),
            self.browse.ptype,
            self.browse.sort,
            self.browse.offset,
        );
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let (gv, loader) = match (&target, filter) {
                (Some(i), true) => (Some(i.mc_version.clone()), Some(i.loader)),
                _ => (None, None),
            };
            let res = modrinth::search(&query, ptype, gv.as_deref(), loader, sort, offset)
                .map_err(|e| format!("{e:#}"));
            let _ = tx.send(Msg::Search(gen, res));
            ctx.request_repaint();
        });
    }

    fn install_hit(&mut self, hit: &SearchHit) {
        let ptype = ProjectType::from_id(&hit.project_type);
        let (pid, title, icon) = (
            hit.project_id.clone(),
            hit.title.clone(),
            hit.icon_url.clone(),
        );
        let task = if ptype == ProjectType::Modpack {
            let tx = self.tx.clone();
            self.tasks
                .spawn(format!("Installing modpack {title}"), move |task| {
                    let id = modrinth::install_modpack(&pid, &title, icon, task)?;
                    let _ = tx.send(Msg::OpenInstance(id));
                    Ok(())
                })
        } else {
            let Some(target) = self.browse.target.clone() else {
                return;
            };
            let inst_name = self
                .instance(&target)
                .map(|i| i.name.clone())
                .unwrap_or_default();
            self.tasks
                .spawn(format!("Installing {title} → {inst_name}"), move |task| {
                    modrinth::install(&target, &pid, &title, icon, ptype, task)
                })
        };
        self.installing.insert(hit.project_id.clone(), task);
    }

    fn create_instance(&mut self) {
        let Some(n) = self.new_instance.take() else {
            return;
        };
        let name = if n.name.trim().is_empty() {
            format!("{} {}", n.loader.label(), n.version)
        } else {
            n.name.clone()
        };
        match Instance::create(&name, &n.version, n.loader) {
            Ok(inst) => {
                tasks::log(format!("[paprika] Created instance {}", inst.name));
                if n.loader != Loader::Vanilla {
                    self.browse.target = Some(inst.id.clone());
                    self.browse.needs_search = true;
                }
                self.reload_instances();
                self.selected = Some(inst.id);
                self.refresh_content();
            }
            Err(e) => tasks::log(format!("[paprika] Couldn't create instance: {e:#}")),
        }
    }

    // ---------- layout ----------

    fn sidebar(&mut self, ui: &mut Ui) {
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::hover());
            theme::paint_logo(ui, rect.center(), 44.0);
            ui.label(
                RichText::new("Paprika Launcher")
                    .size(17.0)
                    .strong()
                    .color(Color32::WHITE),
            );
        });
        ui.add_space(22.0);
        for (page, icon, label) in [
            (Page::Library, "🏠", "Library"),
            (Page::Browse, "🔍", "Browse Modrinth"),
            (Page::Accounts, "👤", "Accounts"),
            (Page::Settings, "⚙", "Settings"),
            (Page::Console, "🖥", "Console"),
        ] {
            let active = self.page == page;
            let text = RichText::new(format!("{icon}   {label}"))
                .size(15.0)
                .color(if active { Color32::WHITE } else { TEXT_DIM });
            let btn = egui::Button::new(text)
                .fill(if active {
                    ACCENT.linear_multiply(0.85)
                } else {
                    Color32::TRANSPARENT
                })
                .stroke(Stroke::NONE)
                .min_size(vec2(ui.available_width(), 38.0));
            if ui.add(btn).clicked() {
                if page == Page::Library && self.page == Page::Library {
                    self.selected = None;
                }
                self.page = page;
            }
        }
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.add_space(12.0);
            ui.label(
                RichText::new(format!("v{} · by Yeeboein", env!("CARGO_PKG_VERSION")))
                    .size(11.0)
                    .color(TEXT_DIM),
            );
            ui.add_space(4.0);
            let acc = self.accounts.active().cloned();
            let resp = Frame::new()
                .fill(CARD)
                .corner_radius(10)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| match &acc {
                        Some(a) => {
                            avatar(ui, a, 28.0);
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&a.username).strong());
                                ui.label(
                                    RichText::new(if a.kind == AccountKind::Microsoft {
                                        "Microsoft"
                                    } else {
                                        "Offline"
                                    })
                                    .size(11.0)
                                    .color(TEXT_DIM),
                                );
                            });
                        }
                        None => {
                            ui.label(RichText::new("No account - click to add").color(ORANGE));
                        }
                    });
                });
            if resp.response.interact(Sense::click()).clicked() {
                self.page = Page::Accounts;
            }
        });
    }

    fn task_bar(&mut self, ui: &mut Ui) {
        for t in self.tasks.list() {
            ui.horizontal(|ui| {
                let err = t.error();
                if let Some(e) = &err {
                    ui.label(RichText::new("✖").color(ACCENT));
                    ui.label(RichText::new(&t.title).strong());
                    ui.label(RichText::new(e).color(Color32::from_rgb(255, 150, 140)));
                    if ui.small_button("Dismiss").clicked() {
                        self.tasks.dismiss(t.id);
                    }
                } else if t.finished_at().is_some() {
                    ui.label(RichText::new("✔").color(GREEN));
                    ui.label(RichText::new(&t.title).strong());
                    ui.label(RichText::new("done").color(TEXT_DIM));
                } else {
                    ui.add(egui::Spinner::new().size(14.0));
                    ui.label(RichText::new(&t.title).strong());
                    ui.label(RichText::new(t.get_status()).color(TEXT_DIM));
                    let bar = match t.fraction() {
                        Some(f) => egui::ProgressBar::new(f).show_percentage(),
                        None => egui::ProgressBar::new(0.0).animate(true),
                    };
                    ui.add(
                        bar.desired_width(ui.available_width().min(320.0))
                            .fill(ACCENT),
                    );
                }
            });
        }
    }

    fn header(ui: &mut Ui, title: &str, subtitle: &str) {
        ui.label(
            RichText::new(title)
                .size(26.0)
                .strong()
                .color(Color32::WHITE),
        );
        if !subtitle.is_empty() {
            ui.label(RichText::new(subtitle).color(TEXT_DIM));
        }
        ui.add_space(8.0);
    }

    fn accent_button(ui: &mut Ui, text: &str, enabled: bool) -> egui::Response {
        let btn = egui::Button::new(RichText::new(text).strong().color(Color32::WHITE))
            .fill(ACCENT)
            .min_size(vec2(0.0, 32.0));
        let r = ui.add_enabled(enabled, btn);
        if r.hovered() {
            ui.painter().rect_stroke(
                r.rect,
                7,
                Stroke::new(1.5_f32, ACCENT_HOVER),
                egui::StrokeKind::Outside,
            );
        }
        r
    }

    fn instance_icon(ui: &mut Ui, inst: &Instance, size: f32) {
        match &inst.icon_url {
            Some(url) => {
                ui.add(
                    egui::Image::new(url.as_str())
                        .fit_to_exact_size(vec2(size, size))
                        .corner_radius(8),
                );
            }
            None => {
                let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
                let bg = match inst.loader {
                    Loader::Vanilla => Color32::from_rgb(52, 60, 48),
                    Loader::Fabric => Color32::from_rgb(64, 52, 44),
                    Loader::Quilt => Color32::from_rgb(58, 46, 66),
                    Loader::Forge => Color32::from_rgb(44, 52, 66),
                    Loader::NeoForge => Color32::from_rgb(66, 50, 40),
                };
                ui.painter().rect_filled(rect, 8, bg);
                theme::paint_logo(ui, rect.center(), size * 0.8);
            }
        }
    }

    // ---------- library ----------

    fn library(&mut self, ui: &mut Ui) {
        if let Some(id) = self.selected.clone() {
            if self.instance(&id).is_some() {
                return self.instance_view(ui, &id);
            }
        }
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                Self::header(
                    ui,
                    "Library",
                    "Your Minecraft instances. Each one keeps its own mods, worlds and settings.",
                )
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if Self::accent_button(ui, "+  New instance", true).clicked() {
                    let version = self
                        .manifest
                        .as_ref()
                        .map(|m| m.latest.release.clone())
                        .unwrap_or_default();
                    self.new_instance = Some(NewInstance {
                        name: String::new(),
                        version,
                        loader: Loader::Fabric,
                        snapshots: false,
                    });
                }
            });
        });
        if self.instances.is_empty() {
            ui.add_space(60.0);
            ui.vertical_centered(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(90.0, 90.0), Sense::hover());
                theme::paint_logo(ui, rect.center(), 90.0);
                ui.label(RichText::new("Nothing cooking yet").size(20.0).strong());
                ui.label(
                    RichText::new("Create an instance, or grab a modpack from Browse Modrinth.")
                        .color(TEXT_DIM),
                );
                ui.add_space(8.0);
                if ui.button("Browse modpacks").clicked() {
                    self.page = Page::Browse;
                    self.browse.ptype = ProjectType::Modpack;
                    self.browse.needs_search = true;
                }
            });
            return;
        }
        let mut action: Option<(String, &'static str)> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(14.0, 14.0);
                    for inst in &self.instances {
                        let running = self.is_running(&inst.id);
                        let launching = self.is_launching(&inst.id);
                        let resp = ui
                            .allocate_ui(vec2(210.0, 230.0), |ui| {
                                let r = Frame::new()
                                    .fill(CARD)
                                    .corner_radius(12)
                                    .inner_margin(12.0)
                                    .show(ui, |ui| {
                                        ui.set_width(186.0);
                                        ui.set_height(206.0);
                                        ui.vertical_centered(|ui| {
                                            Self::instance_icon(ui, inst, 96.0);
                                            ui.add_space(4.0);
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(&inst.name).size(15.0).strong(),
                                                )
                                                .truncate(),
                                            );
                                            ui.label(
                                                RichText::new(inst.label())
                                                    .color(TEXT_DIM)
                                                    .size(12.0),
                                            );
                                            ui.add_space(4.0);
                                            if running {
                                                if ui
                                                    .add(
                                                        egui::Button::new("■  Stop")
                                                            .min_size(vec2(150.0, 30.0)),
                                                    )
                                                    .clicked()
                                                {
                                                    action = Some((inst.id.clone(), "stop"));
                                                }
                                            } else if launching {
                                                ui.add(egui::Spinner::new());
                                            } else if ui
                                                .add(
                                                    egui::Button::new(
                                                        RichText::new("▶  Play")
                                                            .strong()
                                                            .color(Color32::WHITE),
                                                    )
                                                    .fill(ACCENT)
                                                    .min_size(vec2(150.0, 30.0)),
                                                )
                                                .clicked()
                                            {
                                                action = Some((inst.id.clone(), "play"));
                                            }
                                        });
                                    });
                                r.response
                            })
                            .inner;
                        let resp = resp.interact(Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_stroke(
                                resp.rect,
                                12,
                                Stroke::new(1.5_f32, ACCENT.linear_multiply(0.7)),
                                egui::StrokeKind::Inside,
                            );
                        }
                        if resp.clicked() && action.is_none() {
                            action = Some((inst.id.clone(), "open"));
                        }
                    }
                });
            });
        if let Some((id, what)) = action {
            match what {
                "play" => self.play(&id),
                "stop" => self.stop(&id),
                _ => {
                    self.selected = Some(id);
                    self.inst_tab = InstTab::Content;
                    self.refresh_content();
                }
            }
        }
    }

    fn instance_view(&mut self, ui: &mut Ui, id: &str) {
        let inst = self.instance(id).unwrap().clone();
        let running = self.is_running(id);
        let launching = self.is_launching(id);
        if ui.button("⬅  Library").clicked() {
            self.selected = None;
            return;
        }
        ui.add_space(4.0);
        Frame::new()
            .fill(PANEL)
            .corner_radius(14)
            .inner_margin(16.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    Self::instance_icon(ui, &inst, 72.0);
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(&inst.name)
                                .size(24.0)
                                .strong()
                                .color(Color32::WHITE),
                        );
                        let lv = inst
                            .loader_version
                            .as_ref()
                            .map(|v| format!(" · loader {v}"))
                            .unwrap_or_default();
                        ui.label(RichText::new(format!("{}{lv}", inst.label())).color(TEXT_DIM));
                        let mods = self
                            .content_cache
                            .iter()
                            .filter(|c| c.folder == "mods")
                            .count();
                        ui.label(
                            RichText::new(format!("{mods} mods"))
                                .color(TEXT_DIM)
                                .size(12.0),
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if running {
                            if ui
                                .add(
                                    egui::Button::new(RichText::new("■  Stop").size(16.0))
                                        .min_size(vec2(130.0, 42.0)),
                                )
                                .clicked()
                            {
                                self.stop(id);
                            }
                        } else if launching {
                            ui.add(egui::Spinner::new().size(24.0));
                        } else if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("▶  Play")
                                        .size(18.0)
                                        .strong()
                                        .color(Color32::WHITE),
                                )
                                .fill(ACCENT)
                                .min_size(vec2(140.0, 42.0)),
                            )
                            .clicked()
                        {
                            self.play(id);
                        }
                        if ui.button("📂 Folder").clicked() {
                            let _ = open::that(inst.game_dir());
                        }
                    });
                });
            });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.inst_tab,
                InstTab::Content,
                RichText::new("Content").size(15.0),
            );
            ui.selectable_value(
                &mut self.inst_tab,
                InstTab::Options,
                RichText::new("Options").size(15.0),
            );
        });
        ui.separator();
        match self.inst_tab {
            InstTab::Content => self.content_tab(ui, &inst),
            InstTab::Options => self.options_tab(ui, &inst),
        }
    }

    fn content_tab(&mut self, ui: &mut Ui, inst: &Instance) {
        ui.horizontal(|ui| {
            let can_mod = inst.loader != Loader::Vanilla;
            if Self::accent_button(ui, "+  Add content", true).clicked() {
                self.browse.target = Some(inst.id.clone());
                self.browse.ptype = if can_mod {
                    ProjectType::Mod
                } else {
                    ProjectType::ResourcePack
                };
                self.browse.offset = 0;
                self.browse.needs_search = true;
                self.page = Page::Browse;
            }
            if ui
                .add_enabled(can_mod, egui::Button::new("⟳  Update all mods"))
                .clicked()
            {
                let id = inst.id.clone();
                self.tasks
                    .spawn(format!("Updating mods in {}", inst.name), move |task| {
                        let n = modrinth::update_all(&id, task)?;
                        task.status(format!("{n} updated"));
                        Ok(())
                    });
            }
            if ui.button("Refresh").clicked() {
                self.refresh_content();
            }
        });
        if inst.loader == Loader::Vanilla {
            ui.label(RichText::new("This is a vanilla instance. Make a Fabric, Quilt, Forge or NeoForge instance to use mods; resource packs and shaders still work here.").color(TEXT_DIM));
        }
        ui.add_space(4.0);
        if self.content_cache.is_empty() {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("No content installed yet").size(17.0).strong());
                ui.label(RichText::new("Hit “Add content” to browse thousands of mods on Modrinth - one click installs them.").color(TEXT_DIM));
            });
            return;
        }
        let mut toggle = None;
        let mut remove = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (i, c) in self.content_cache.iter().enumerate() {
                    Frame::new()
                        .fill(if i % 2 == 0 { CARD } else { PANEL })
                        .corner_radius(8)
                        .inner_margin(egui::Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                let mut on = c.enabled;
                                if ui
                                    .checkbox(&mut on, "")
                                    .on_hover_text("Enable / disable")
                                    .changed()
                                {
                                    toggle = Some(i);
                                }
                                match c.meta.as_ref().and_then(|m| m.icon_url.clone()) {
                                    Some(url) => {
                                        ui.add(
                                            egui::Image::new(url)
                                                .fit_to_exact_size(vec2(30.0, 30.0))
                                                .corner_radius(6),
                                        );
                                    }
                                    None => {
                                        let (rect, _) = ui
                                            .allocate_exact_size(vec2(30.0, 30.0), Sense::hover());
                                        ui.painter().rect_filled(rect, 6, CARD_HOVER);
                                        ui.painter().text(
                                            rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            "📦",
                                            egui::FontId::proportional(15.0),
                                            TEXT_DIM,
                                        );
                                    }
                                }
                                ui.vertical(|ui| {
                                    let name = RichText::new(c.display_name()).strong();
                                    ui.label(if c.enabled {
                                        name
                                    } else {
                                        name.strikethrough().color(TEXT_DIM)
                                    });
                                    let sub = match &c.meta {
                                        Some(m) => format!("{} · {}", m.version_number, c.filename),
                                        None => c.filename.clone(),
                                    };
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(sub).size(11.0).color(TEXT_DIM),
                                        )
                                        .truncate(),
                                    );
                                });
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.button("🗑").on_hover_text("Remove").clicked() {
                                        remove = Some(i);
                                    }
                                    let tag = match c.folder {
                                        "mods" => ("mod", ORANGE),
                                        "resourcepacks" => {
                                            ("resource pack", Color32::from_rgb(110, 170, 240))
                                        }
                                        _ => ("shader", Color32::from_rgb(190, 130, 240)),
                                    };
                                    ui.label(RichText::new(tag.0).size(11.0).color(tag.1));
                                });
                            });
                        });
                }
            });
        if let Some(i) = toggle {
            let c = self.content_cache[i].clone();
            if let Err(e) = instance::toggle_content(&inst.id, &c) {
                tasks::log(format!("[paprika] {e:#}"));
            }
            self.refresh_content();
        }
        if let Some(i) = remove {
            let c = self.content_cache[i].clone();
            if let Err(e) = instance::remove_content(&inst.id, &c) {
                tasks::log(format!("[paprika] {e:#}"));
            }
            self.reload_instances();
        }
    }

    fn options_tab(&mut self, ui: &mut Ui, inst: &Instance) {
        egui::Grid::new("inst_opts")
            .num_columns(2)
            .spacing(vec2(16.0, 12.0))
            .show(ui, |ui| {
                ui.label("Name");
                ui.horizontal(|ui| {
                    if self.rename_buf.is_empty() {
                        self.rename_buf = inst.name.clone();
                    }
                    ui.add(egui::TextEdit::singleline(&mut self.rename_buf).desired_width(240.0));
                    if ui
                        .add_enabled(
                            self.rename_buf.trim() != inst.name
                                && !self.rename_buf.trim().is_empty(),
                            egui::Button::new("Rename"),
                        )
                        .clicked()
                    {
                        let n = self.rename_buf.trim().to_string();
                        let _ = Instance::modify(&inst.id, |i| i.name = n);
                    }
                });
                ui.end_row();

                ui.label("Memory");
                ui.horizontal(|ui| {
                    let mut custom = inst.memory_mb.is_some();
                    let mut mem = inst.memory_mb.unwrap_or(self.settings.memory_mb);
                    let mut changed = ui.checkbox(&mut custom, "Custom").changed();
                    if custom {
                        changed |= ui
                            .add(
                                egui::Slider::new(&mut mem, 1024..=16384)
                                    .step_by(256.0)
                                    .suffix(" MB"),
                            )
                            .changed();
                    } else {
                        ui.label(
                            RichText::new(format!(
                                "Using default ({} MB)",
                                self.settings.memory_mb
                            ))
                            .color(TEXT_DIM),
                        );
                    }
                    if changed {
                        let v = custom.then_some(mem);
                        let _ = Instance::modify(&inst.id, |i| i.memory_mb = v);
                    }
                });
                ui.end_row();

                ui.label("Version");
                ui.label(format!(
                    "Minecraft {} · {}",
                    inst.mc_version,
                    inst.loader.label()
                ));
                ui.end_row();

                if inst.loader != Loader::Vanilla {
                    ui.label("Loader version");
                    ui.horizontal(|ui| {
                        ui.label(
                            inst.loader_version
                                .clone()
                                .unwrap_or_else(|| "newest stable (picked at first launch)".into()),
                        );
                        if inst.loader_version.is_some()
                            && ui
                                .small_button("Use newest")
                                .on_hover_text("Pick the newest stable loader on next launch")
                                .clicked()
                        {
                            let _ = Instance::modify(&inst.id, |i| i.loader_version = None);
                        }
                    });
                    ui.end_row();
                }

                ui.label("Folder");
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(inst.dir().display().to_string())
                            .monospace()
                            .size(11.0)
                            .color(TEXT_DIM),
                    );
                    if ui.small_button("Open").clicked() {
                        let _ = open::that(inst.dir());
                    }
                });
                ui.end_row();
            });
        ui.add_space(20.0);
        if self.confirm_delete.as_deref() == Some(inst.id.as_str()) {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "Delete “{}” and all its worlds forever?",
                        inst.name
                    ))
                    .color(ACCENT),
                );
                if ui
                    .add(
                        egui::Button::new(RichText::new("Yes, delete").color(Color32::WHITE))
                            .fill(ACCENT),
                    )
                    .clicked()
                {
                    if let Err(e) = Instance::delete(&inst.id) {
                        tasks::log(format!("[paprika] {e:#}"));
                    }
                    self.confirm_delete = None;
                    self.selected = None;
                    self.reload_instances();
                }
                if ui.button("Cancel").clicked() {
                    self.confirm_delete = None;
                }
            });
        } else if ui
            .button(RichText::new("🗑  Delete instance").color(Color32::from_rgb(255, 140, 130)))
            .clicked()
        {
            self.confirm_delete = Some(inst.id.clone());
        }
    }

    // ---------- browse ----------

    fn browse_page(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        Self::header(ui, "Browse Modrinth", "Find mods, modpacks, resource packs and shaders. Click Install and they go straight into your instance.");
        let mut search = false;
        ui.horizontal(|ui| {
            for t in ProjectType::ALL {
                if ui
                    .selectable_label(self.browse.ptype == t, RichText::new(t.label()).size(15.0))
                    .clicked()
                    && self.browse.ptype != t
                {
                    self.browse.ptype = t;
                    search = true;
                }
            }
        });
        ui.horizontal(|ui| {
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.browse.query)
                    .hint_text("🔍  Search Modrinth…")
                    .desired_width(300.0)
                    .min_size(vec2(0.0, 30.0)),
            );
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                search = true;
            }
            if r.changed() && self.browse.query.is_empty() {
                search = true;
            }
            egui::ComboBox::from_id_salt("sort")
                .selected_text(format!("Sort: {}", self.browse.sort.label()))
                .show_ui(ui, |ui| {
                    for s in Sort::ALL {
                        if ui
                            .selectable_value(&mut self.browse.sort, s, s.label())
                            .changed()
                        {
                            search = true;
                        }
                    }
                });
            if self.browse.ptype != ProjectType::Modpack {
                let label = self
                    .browse
                    .target
                    .as_ref()
                    .and_then(|t| self.instance(t))
                    .map(|i| format!("{} ({})", i.name, i.label()))
                    .unwrap_or_else(|| "Pick an instance".into());
                egui::ComboBox::from_id_salt("target")
                    .selected_text(format!("Install into: {label}"))
                    .width(260.0)
                    .show_ui(ui, |ui| {
                        for inst in &self.instances {
                            if ui
                                .selectable_label(
                                    self.browse.target.as_deref() == Some(&inst.id),
                                    format!("{} ({})", inst.name, inst.label()),
                                )
                                .clicked()
                            {
                                self.browse.target = Some(inst.id.clone());
                                search = true;
                            }
                        }
                    });
            }
            if ui.button("Search").clicked() {
                search = true;
            }
        });
        if search {
            self.browse.offset = 0;
            self.browse.needs_search = true;
        }
        if self.browse.needs_search {
            self.start_search(&ctx);
        }

        let target = self
            .browse
            .target
            .as_ref()
            .and_then(|t| self.instance(t))
            .cloned();
        if self.browse.ptype != ProjectType::Modpack {
            match &target {
                None => {
                    ui.label(RichText::new("Pick (or create) an instance above to install into - results are filtered to what works with it.").color(ORANGE));
                }
                Some(i) if i.loader == Loader::Vanilla && self.browse.ptype == ProjectType::Mod => {
                    ui.label(
                        RichText::new(format!(
                            "“{}” is vanilla - mods need a mod loader (Fabric, Quilt, Forge or NeoForge).",
                            i.name
                        ))
                        .color(ORANGE),
                    );
                }
                Some(i) => {
                    ui.label(
                        RichText::new(format!(
                            "Showing what works with {} · {}",
                            i.name,
                            i.label()
                        ))
                        .color(TEXT_DIM)
                        .size(12.0),
                    );
                }
            }
        }
        if let Some(e) = &self.browse.error {
            ui.label(RichText::new(format!("Couldn't reach Modrinth: {e}")).color(ACCENT));
        }
        ui.add_space(4.0);

        // Finished installs no longer count as "installing".
        self.installing.retain(|_, t| t.finished_at().is_none());

        let installed: std::collections::HashSet<String> = target
            .as_ref()
            .map(|i| i.content.iter().map(|c| c.project_id.clone()).collect())
            .unwrap_or_default();
        let mut clicked: Option<SearchHit> = None;
        let mut page_change: Option<u64> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.browse.loading && self.browse.results.is_empty() {
                    ui.add_space(40.0);
                    ui.vertical_centered(|ui| ui.add(egui::Spinner::new().size(32.0)));
                }
                for hit in &self.browse.results {
                    Frame::new()
                        .fill(CARD)
                        .corner_radius(12)
                        .inner_margin(12.0)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                match &hit.icon_url {
                                    Some(u) if !u.is_empty() => {
                                        ui.add(
                                            egui::Image::new(u.as_str())
                                                .fit_to_exact_size(vec2(64.0, 64.0))
                                                .corner_radius(10),
                                        );
                                    }
                                    _ => {
                                        let (rect, _) = ui
                                            .allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
                                        ui.painter().rect_filled(rect, 10, CARD_HOVER);
                                        theme::paint_logo(ui, rect.center(), 46.0);
                                    }
                                }
                                ui.vertical(|ui| {
                                    ui.set_max_width(ui.available_width() - 150.0);
                                    ui.horizontal(|ui| {
                                        ui.hyperlink_to(
                                            RichText::new(&hit.title)
                                                .size(17.0)
                                                .strong()
                                                .color(Color32::WHITE),
                                            format!(
                                                "https://modrinth.com/{}/{}",
                                                hit.project_type, hit.slug
                                            ),
                                        );
                                        ui.label(
                                            RichText::new(format!("by {}", hit.author))
                                                .color(TEXT_DIM),
                                        );
                                    });
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&hit.description)
                                                .color(Color32::from_rgb(215, 205, 210)),
                                        )
                                        .wrap(),
                                    );
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(format!(
                                                "⬇ {}",
                                                theme::fmt_count(hit.downloads)
                                            ))
                                            .size(12.0)
                                            .color(TEXT_DIM),
                                        );
                                        ui.label(
                                            RichText::new(format!(
                                                "♥ {}",
                                                theme::fmt_count(hit.follows)
                                            ))
                                            .size(12.0)
                                            .color(TEXT_DIM),
                                        );
                                        for c in hit.categories.iter().take(4) {
                                            ui.label(RichText::new(c).size(11.0).color(ORANGE));
                                        }
                                    });
                                });
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    let is_pack = hit.project_type == "modpack";
                                    if self.installing.contains_key(&hit.project_id) {
                                        ui.add(egui::Spinner::new());
                                        ui.label("Installing…");
                                    } else if !is_pack && installed.contains(&hit.project_id) {
                                        ui.label(
                                            RichText::new("✔ Installed").color(GREEN).strong(),
                                        );
                                    } else {
                                        let can = is_pack
                                            || target.as_ref().is_some_and(|t| {
                                                t.loader != Loader::Vanilla
                                                    || hit.project_type != "mod"
                                            });
                                        let label = if is_pack {
                                            "⬇  Install pack"
                                        } else {
                                            "⬇  Install"
                                        };
                                        if Self::accent_button(ui, label, can).clicked() {
                                            clicked = Some(hit.clone());
                                        }
                                    }
                                });
                            });
                        });
                    ui.add_space(2.0);
                }
                if !self.browse.results.is_empty() {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let page = self.browse.offset / modrinth::PAGE_SIZE + 1;
                        let pages = self.browse.total.div_ceil(modrinth::PAGE_SIZE).max(1);
                        if ui
                            .add_enabled(self.browse.offset > 0, egui::Button::new("← Prev"))
                            .clicked()
                        {
                            page_change =
                                Some(self.browse.offset.saturating_sub(modrinth::PAGE_SIZE));
                        }
                        ui.label(format!(
                            "Page {page} of {pages}  ·  {} results",
                            self.browse.total
                        ));
                        if ui
                            .add_enabled(page < pages, egui::Button::new("Next →"))
                            .clicked()
                        {
                            page_change = Some(self.browse.offset + modrinth::PAGE_SIZE);
                        }
                        if self.browse.loading {
                            ui.add(egui::Spinner::new());
                        }
                    });
                } else if !self.browse.loading && self.browse.error.is_none() {
                    ui.add_space(30.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("No results. Try another search.").color(TEXT_DIM))
                    });
                }
            });
        if let Some(hit) = clicked {
            self.install_hit(&hit);
        }
        if let Some(off) = page_change {
            self.browse.offset = off;
            self.start_search(&ctx);
        }
    }

    // ---------- accounts ----------

    fn accounts_page(&mut self, ui: &mut Ui) {
        Self::header(ui, "Accounts", "Sign in with Microsoft to play online, or add an offline name for singleplayer and LAN.");
        let mut remove = None;
        let mut activate = None;
        for a in &self.accounts.accounts {
            let active = self.accounts.active.as_deref() == Some(&a.uuid);
            Frame::new()
                .fill(CARD)
                .corner_radius(10)
                .inner_margin(10.0)
                .stroke(if active {
                    Stroke::new(1.5_f32, ACCENT)
                } else {
                    Stroke::NONE
                })
                .show(ui, |ui| {
                    ui.set_width(ui.available_width().min(560.0));
                    ui.horizontal(|ui| {
                        avatar(ui, a, 40.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&a.username).size(16.0).strong());
                            ui.label(
                                RichText::new(if a.kind == AccountKind::Microsoft {
                                    "Microsoft account"
                                } else {
                                    "Offline account"
                                })
                                .color(TEXT_DIM)
                                .size(12.0),
                            );
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button("Remove").clicked() {
                                remove = Some(a.uuid.clone());
                            }
                            if active {
                                ui.label(RichText::new("✔ Active").color(GREEN));
                            } else if ui.button("Use").clicked() {
                                activate = Some(a.uuid.clone());
                            }
                        });
                    });
                });
        }
        if let Some(u) = activate {
            self.accounts.active = Some(u);
            config::save_accounts(&self.accounts);
        }
        if let Some(u) = remove {
            self.accounts.accounts.retain(|a| a.uuid != u);
            if self.accounts.active.as_deref() == Some(&u) {
                self.accounts.active = self.accounts.accounts.first().map(|a| a.uuid.clone());
            }
            config::save_accounts(&self.accounts);
        }
        ui.add_space(16.0);

        ui.label(RichText::new("Microsoft").size(17.0).strong());
        let client_id = self.settings.client_id();
        match &mut self.ms_login {
            None => {
                if client_id.is_none() {
                    ui.label(RichText::new("Add a Microsoft (Azure) client ID in Settings to turn on Microsoft sign-in.").color(ORANGE));
                }
                if Self::accent_button(ui, "Sign in with Microsoft", client_id.is_some()).clicked()
                {
                    let cancel = Arc::new(AtomicBool::new(false));
                    self.ms_login = Some(MsLogin {
                        code: None,
                        cancel: cancel.clone(),
                        error: None,
                    });
                    let tx = self.tx.clone();
                    let cid = client_id.unwrap();
                    std::thread::spawn(move || {
                        let res = (|| {
                            let dc = auth::start_device_code(&cid)?;
                            let _ = tx.send(Msg::DeviceCode(dc.clone()));
                            tasks::repaint();
                            auth::finish_device_code(&cid, &dc, &cancel)
                        })();
                        let _ = tx.send(Msg::Login(res.map_err(|e| format!("{e:#}"))));
                        tasks::repaint();
                    });
                }
            }
            Some(login) => {
                let mut close = false;
                Frame::new()
                    .fill(PANEL)
                    .corner_radius(12)
                    .inner_margin(14.0)
                    .show(ui, |ui| {
                        ui.set_max_width(560.0);
                        if let Some(err) = &login.error {
                            ui.label(RichText::new(err).color(ACCENT));
                            if ui.button("OK").clicked() {
                                close = true;
                            }
                        } else if let Some(dc) = &login.code {
                            ui.label("A browser window opened. Enter this code there:");
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(&dc.user_code)
                                        .size(30.0)
                                        .strong()
                                        .monospace()
                                        .color(ORANGE),
                                );
                                if ui.button("📋 Copy").clicked() {
                                    ui.ctx().copy_text(dc.user_code.clone());
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.hyperlink_to(&dc.verification_uri, &dc.verification_uri);
                                ui.add(egui::Spinner::new());
                                ui.label(
                                    RichText::new("waiting for you to finish…").color(TEXT_DIM),
                                );
                            });
                            if ui.button("Cancel").clicked() {
                                login.cancel.store(true, Ordering::SeqCst);
                                close = true;
                            }
                        } else {
                            ui.horizontal(|ui| {
                                ui.add(egui::Spinner::new());
                                ui.label("Contacting Microsoft…");
                            });
                        }
                    });
                if close {
                    self.ms_login = None;
                }
            }
        }
        ui.add_space(16.0);
        ui.label(RichText::new("Offline").size(17.0).strong());
        ui.horizontal(|ui| {
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.offline_name)
                    .hint_text("Player name")
                    .desired_width(220.0)
                    .char_limit(16),
            );
            let valid = (3..=16).contains(&self.offline_name.len())
                && self
                    .offline_name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_');
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui
                .add_enabled(valid, egui::Button::new("Add offline account"))
                .clicked()
                || enter && valid)
                && valid
            {
                self.accounts.upsert(auth::offline(&self.offline_name));
                config::save_accounts(&self.accounts);
                self.offline_name.clear();
            }
        });
        ui.label(
            RichText::new(
                "3-16 letters, numbers or _. Offline accounts can't join online-mode servers.",
            )
            .size(12.0)
            .color(TEXT_DIM),
        );
    }

    // ---------- settings ----------

    fn settings_page(&mut self, ui: &mut Ui) {
        Self::header(ui, "Settings", "");
        let mut changed = false;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("settings").num_columns(2).spacing(vec2(18.0, 14.0)).show(ui, |ui| {
                ui.label(RichText::new("Default memory").strong());
                changed |= ui.add(egui::Slider::new(&mut self.settings.memory_mb, 1024..=16384).step_by(256.0).suffix(" MB")).changed();
                ui.end_row();

                ui.label(RichText::new("Window size").strong());
                ui.horizontal(|ui| {
                    changed |= ui.add(egui::DragValue::new(&mut self.settings.width).range(640..=7680)).changed();
                    ui.label("×");
                    changed |= ui.add(egui::DragValue::new(&mut self.settings.height).range(480..=4320)).changed();
                });
                ui.end_row();

                ui.label(RichText::new("Java").strong());
                ui.vertical(|ui| {
                    changed |= ui.add(egui::TextEdit::singleline(&mut self.settings.java_path).hint_text("Automatic (Paprika downloads the right Java)").desired_width(380.0)).changed();
                    ui.label(RichText::new("Leave empty and Paprika fetches the exact Java each Minecraft version needs.").size(12.0).color(TEXT_DIM));
                });
                ui.end_row();

                ui.label(RichText::new("Extra JVM arguments").strong());
                changed |= ui.add(egui::TextEdit::singleline(&mut self.settings.jvm_args).hint_text("-XX:+UseG1GC …").desired_width(380.0)).changed();
                ui.end_row();

                ui.label(RichText::new("Microsoft client ID").strong());
                ui.vertical(|ui| {
                    changed |= ui.add(egui::TextEdit::singleline(&mut self.settings.msa_client_id).hint_text("Azure application (client) ID").desired_width(380.0)).changed();
                    ui.label(RichText::new("Needed for Microsoft sign-in. Register an app at portal.azure.com (personal accounts, public client flows on) - see the README.").size(12.0).color(TEXT_DIM));
                });
                ui.end_row();

                ui.label(RichText::new("Data folder").strong());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(crate::paths::root().display().to_string()).monospace().size(11.0).color(TEXT_DIM));
                    if ui.small_button("Open").clicked() {
                        let _ = open::that(crate::paths::root());
                    }
                });
                ui.end_row();
            });
            ui.add_space(24.0);
            ui.separator();
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
                theme::paint_logo(ui, rect.center(), 34.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(format!("Paprika Launcher {}", env!("CARGO_PKG_VERSION"))).strong());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Made by Yeeboein ·").color(TEXT_DIM));
                        ui.hyperlink_to("GitHub", "https://github.com/yeeboein/paprika-launcher");
                        ui.hyperlink_to("yeeboein.com", "https://yeeboein.com");
                    });
                    ui.label(RichText::new("Mods by Modrinth. Not an official Minecraft product; not affiliated with Mojang or Microsoft.").size(11.0).color(TEXT_DIM));
                });
            });
        });
        if changed {
            config::save_settings(&self.settings);
        }
    }

    // ---------- console ----------

    fn console_page(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| Self::header(ui, "Console", "Launcher and game output."));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Clear").clicked() {
                    tasks::log_clear();
                }
                if ui.button("📋 Copy all").clicked() {
                    ui.ctx().copy_text(tasks::log_snapshot().join("\n"));
                }
            });
        });
        let lines = tasks::log_snapshot();
        Frame::new()
            .fill(Color32::from_rgb(12, 10, 11))
            .corner_radius(10)
            .inner_margin(10.0)
            .show(ui, |ui| {
                let row = 15.0;
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show_rows(ui, row, lines.len(), |ui, range| {
                        for line in &lines[range] {
                            let color = if line.starts_with("[paprika]") {
                                ORANGE
                            } else if line.contains("ERROR") || line.contains("Exception") {
                                Color32::from_rgb(255, 120, 110)
                            } else if line.contains("WARN") {
                                Color32::from_rgb(240, 200, 100)
                            } else {
                                Color32::from_rgb(200, 195, 198)
                            };
                            ui.label(RichText::new(line).monospace().size(12.0).color(color));
                        }
                    });
            });
    }

    fn new_instance_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let mut create = false;
        let manifest = self.manifest.clone();
        let manifest_error = self.manifest_error.clone();
        let Some(n) = &mut self.new_instance else {
            return;
        };
        egui::Window::new("New instance")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_width(380.0);
                egui::Grid::new("new_inst")
                    .num_columns(2)
                    .spacing(vec2(12.0, 12.0))
                    .show(ui, |ui| {
                        ui.label("Name");
                        ui.add(
                            egui::TextEdit::singleline(&mut n.name)
                                .hint_text(format!("{} {}", n.loader.label(), n.version))
                                .desired_width(240.0),
                        );
                        ui.end_row();
                        ui.label("Loader");
                        ui.horizontal(|ui| {
                            for l in Loader::ALL {
                                ui.selectable_value(&mut n.loader, l, l.label());
                            }
                        });
                        ui.end_row();
                        ui.label("Minecraft");
                        ui.horizontal(|ui| match &manifest {
                            Some(m) => {
                                egui::ComboBox::from_id_salt("mcver")
                                    .selected_text(&n.version)
                                    .width(140.0)
                                    .height(320.0)
                                    .show_ui(ui, |ui| {
                                        for v in m
                                            .versions
                                            .iter()
                                            .filter(|v| n.snapshots || v.kind == "release")
                                        {
                                            ui.selectable_value(
                                                &mut n.version,
                                                v.id.clone(),
                                                &v.id,
                                            );
                                        }
                                    });
                                ui.checkbox(&mut n.snapshots, "Snapshots");
                            }
                            None => {
                                if let Some(e) = &manifest_error {
                                    ui.label(
                                        RichText::new(format!("Can't load versions: {e}"))
                                            .color(ACCENT),
                                    );
                                } else {
                                    ui.add(egui::Spinner::new());
                                    ui.label("Loading versions…");
                                }
                            }
                        });
                        ui.end_row();
                    });
                if n.loader != Loader::Vanilla {
                    ui.label(
                        RichText::new(format!(
                            "The newest stable {} loader is picked automatically.",
                            n.loader.label()
                        ))
                        .size(12.0)
                        .color(TEXT_DIM),
                    );
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if Self::accent_button(ui, "Create", !n.version.is_empty()).clicked() {
                            create = true;
                        }
                    });
                });
            });
        if create {
            self.create_instance();
        } else if !open {
            self.new_instance = None;
        }
    }
}

/// Skin head for Microsoft accounts; a letter tile for offline ones.
fn avatar(ui: &mut Ui, a: &Account, size: f32) {
    if a.kind == AccountKind::Microsoft {
        ui.add(
            egui::Image::new(format!("https://mc-heads.net/avatar/{}/64", a.uuid))
                .fit_to_exact_size(vec2(size, size))
                .corner_radius(5),
        );
    } else {
        let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
        ui.painter()
            .rect_filled(rect, 5, ORANGE.linear_multiply(0.85));
        let letter = a
            .username
            .chars()
            .next()
            .unwrap_or('?')
            .to_ascii_uppercase()
            .to_string();
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            letter,
            egui::FontId::proportional(size * 0.55),
            Color32::WHITE,
        );
    }
}

impl eframe::App for PaprikaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_messages();
        if tasks::take_instances_dirty() {
            self.reload_instances();
        }
        self.tasks.prune();
        let has_tasks = !self.tasks.list().is_empty();
        if has_tasks || !self.running.lock().unwrap().is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }

        egui::SidePanel::left("nav")
            .exact_width(210.0)
            .resizable(false)
            .frame(
                Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(12, 0)),
            )
            .show(ctx, |ui| self.sidebar(ui));

        if has_tasks {
            egui::TopBottomPanel::bottom("tasks")
                .frame(
                    Frame::new()
                        .fill(PANEL)
                        .inner_margin(egui::Margin::symmetric(16, 10)),
                )
                .show(ctx, |ui| self.task_bar(ui));
        }

        egui::CentralPanel::default()
            .frame(Frame::new().fill(theme::BG).inner_margin(22.0))
            .show(ctx, |ui| match self.page {
                Page::Library => self.library(ui),
                Page::Browse => self.browse_page(ui),
                Page::Accounts => self.accounts_page(ui),
                Page::Settings => self.settings_page(ui),
                Page::Console => self.console_page(ui),
            });

        self.new_instance_window(ctx);
    }
}
