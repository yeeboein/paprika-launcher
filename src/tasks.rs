//! Background tasks with progress, plus the launcher console log.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

static CTX: OnceLock<eframe::egui::Context> = OnceLock::new();
static INSTANCES_DIRTY: AtomicBool = AtomicBool::new(false);
static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());
const LOG_CAP: usize = 8000;

pub fn set_ctx(ctx: eframe::egui::Context) {
    let _ = CTX.set(ctx);
}

/// Ask the UI to redraw (safe from any thread).
pub fn repaint() {
    if let Some(ctx) = CTX.get() {
        ctx.request_repaint();
    }
}

/// Mark instance data as changed on disk so the UI reloads it.
pub fn mark_instances_dirty() {
    INSTANCES_DIRTY.store(true, Ordering::SeqCst);
    repaint();
}

pub fn take_instances_dirty() -> bool {
    INSTANCES_DIRTY.swap(false, Ordering::SeqCst)
}

pub fn log(line: impl Into<String>) {
    let line = line.into();
    let mut l = LOG.lock().unwrap();
    l.push(line);
    if l.len() > LOG_CAP {
        let extra = l.len() - LOG_CAP;
        l.drain(0..extra);
    }
    drop(l);
    repaint();
}

pub fn log_snapshot() -> Vec<String> {
    LOG.lock().unwrap().clone()
}

pub fn log_clear() {
    LOG.lock().unwrap().clear();
}

pub struct Task {
    pub id: u64,
    pub title: String,
    status: Mutex<String>,
    done: AtomicUsize,
    total: AtomicUsize,
    finished: Mutex<Option<Instant>>,
    error: Mutex<Option<String>>,
}

impl Task {
    pub fn status(&self, s: impl Into<String>) {
        *self.status.lock().unwrap() = s.into();
        repaint();
    }
    pub fn get_status(&self) -> String {
        self.status.lock().unwrap().clone()
    }
    pub fn set_progress(&self, done: usize, total: usize) {
        self.total.store(total, Ordering::SeqCst);
        self.done.store(done, Ordering::SeqCst);
        repaint();
    }
    pub fn inc(&self) {
        self.done.fetch_add(1, Ordering::SeqCst);
        repaint();
    }
    /// Progress between 0 and 1, or None when unknown.
    pub fn fraction(&self) -> Option<f32> {
        let t = self.total.load(Ordering::SeqCst);
        if t == 0 {
            return None;
        }
        Some((self.done.load(Ordering::SeqCst) as f32 / t as f32).min(1.0))
    }
    pub fn finished_at(&self) -> Option<Instant> {
        *self.finished.lock().unwrap()
    }
    pub fn error(&self) -> Option<String> {
        self.error.lock().unwrap().clone()
    }
}

pub type TaskRef = Arc<Task>;

#[derive(Clone, Default)]
pub struct TaskManager {
    tasks: Arc<Mutex<Vec<TaskRef>>>,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

impl TaskManager {
    pub fn spawn<F>(&self, title: impl Into<String>, f: F) -> TaskRef
    where
        F: FnOnce(&TaskRef) -> anyhow::Result<()> + Send + 'static,
    {
        let task = Arc::new(Task {
            id: NEXT_ID.fetch_add(1, Ordering::SeqCst),
            title: title.into(),
            status: Mutex::new("Starting…".into()),
            done: AtomicUsize::new(0),
            total: AtomicUsize::new(0),
            finished: Mutex::new(None),
            error: Mutex::new(None),
        });
        self.tasks.lock().unwrap().push(task.clone());
        let t = task.clone();
        std::thread::spawn(move || {
            let res = f(&t);
            if let Err(e) = res {
                let msg = format!("{e:#}");
                log(format!("[paprika] {} failed: {msg}", t.title));
                *t.error.lock().unwrap() = Some(msg);
            }
            *t.finished.lock().unwrap() = Some(Instant::now());
            mark_instances_dirty();
        });
        task
    }

    pub fn list(&self) -> Vec<TaskRef> {
        self.tasks.lock().unwrap().clone()
    }

    /// Drop finished tasks: successful ones after a few seconds, failed ones
    /// only when dismissed.
    pub fn prune(&self) {
        self.tasks
            .lock()
            .unwrap()
            .retain(|t| match t.finished_at() {
                None => true,
                Some(at) => t.error().is_some() || at.elapsed().as_secs() < 3,
            });
    }

    pub fn dismiss(&self, id: u64) {
        self.tasks.lock().unwrap().retain(|t| t.id != id);
    }
}
