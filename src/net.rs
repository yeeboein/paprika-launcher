//! HTTP helpers: JSON fetches and verified, parallel file downloads.

use crate::tasks::TaskRef;
use anyhow::{anyhow, bail, Context, Result};
use serde::de::DeserializeOwned;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

pub const USER_AGENT: &str = concat!(
    "yeeboein/paprika-launcher/",
    env!("CARGO_PKG_VERSION"),
    " (yeeboein.com)"
);

pub fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::AgentBuilder::new()
            .user_agent(USER_AGENT)
            .timeout_connect(Duration::from_secs(15))
            .timeout_read(Duration::from_secs(60))
            .try_proxy_from_env(true)
            .build()
    })
}

/// Turn a ureq error into a readable message, including the response body.
pub fn describe(e: ureq::Error) -> anyhow::Error {
    match e {
        ureq::Error::Status(code, resp) => {
            let url = resp.get_url().to_string();
            let body = resp.into_string().unwrap_or_default();
            let body: String = body.chars().take(300).collect();
            anyhow!("HTTP {code} from {url}: {body}")
        }
        other => anyhow!("{other}"),
    }
}

pub fn get_json<T: DeserializeOwned>(url: &str) -> Result<T> {
    let resp = agent().get(url).call().map_err(describe)?;
    resp.into_json::<T>()
        .with_context(|| format!("bad JSON from {url}"))
}

pub fn get_string(url: &str) -> Result<String> {
    let resp = agent().get(url).call().map_err(describe)?;
    Ok(resp.into_string()?)
}

#[derive(Clone, Debug)]
pub struct Download {
    pub url: String,
    pub path: PathBuf,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    pub executable: bool,
}

impl Download {
    pub fn new(url: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Download {
            url: url.into(),
            path: path.into(),
            sha1: None,
            size: None,
            executable: false,
        }
    }
    pub fn sha1(mut self, sha1: Option<String>) -> Self {
        self.sha1 = sha1;
        self
    }
    pub fn size(mut self, size: Option<u64>) -> Self {
        self.size = size;
        self
    }
}

pub fn file_sha1(path: &Path) -> Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = sha1_smol::Sha1::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.digest().to_string())
}

/// Is the file already there and (cheaply) looking right?
fn already_ok(d: &Download) -> bool {
    match std::fs::metadata(&d.path) {
        Ok(m) if m.is_file() => match d.size {
            Some(s) => m.len() == s,
            None => match &d.sha1 {
                Some(h) => file_sha1(&d.path)
                    .map(|x| x.eq_ignore_ascii_case(h))
                    .unwrap_or(false),
                None => true,
            },
        },
        _ => false,
    }
}

fn download_once(d: &Download) -> Result<()> {
    if let Some(parent) = d.path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let resp = agent().get(&d.url).call().map_err(describe)?;
    let tmp = d.path.with_extension("paprika-part");
    let mut out = std::fs::File::create(&tmp)?;
    let mut reader = resp.into_reader();
    let mut h = sha1_smol::Sha1::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        out.write_all(&buf[..n])?;
    }
    out.flush()?;
    drop(out);
    if let Some(expected) = &d.sha1 {
        let got = h.digest().to_string();
        if !got.eq_ignore_ascii_case(expected) {
            let _ = std::fs::remove_file(&tmp);
            bail!(
                "checksum mismatch for {} (expected {expected}, got {got})",
                d.url
            );
        }
    }
    std::fs::rename(&tmp, &d.path)?;
    #[cfg(unix)]
    if d.executable {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&d.path, std::fs::Permissions::from_mode(0o755));
    }
    Ok(())
}

pub fn download(d: &Download) -> Result<()> {
    if already_ok(d) {
        return Ok(());
    }
    let mut last = None;
    for attempt in 0..3 {
        match download_once(d) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(Duration::from_millis(400 * (attempt + 1)));
            }
        }
    }
    Err(last.unwrap()).with_context(|| format!("downloading {}", d.url))
}

/// Download everything in parallel, updating the task's progress.
pub fn download_all(jobs: Vec<Download>, task: &TaskRef) -> Result<()> {
    let jobs: Vec<Download> = jobs.into_iter().filter(|d| !already_ok(d)).collect();
    if jobs.is_empty() {
        return Ok(());
    }
    let total = jobs.len();
    task.set_progress(0, total);
    let queue = Arc::new(Mutex::new(jobs));
    let errors: Arc<Mutex<Vec<String>>> = Arc::default();
    let workers = 12.min(total);
    let mut handles = Vec::new();
    for _ in 0..workers {
        let queue = queue.clone();
        let errors = errors.clone();
        let task = task.clone();
        handles.push(std::thread::spawn(move || loop {
            let job = queue.lock().unwrap().pop();
            let Some(job) = job else { break };
            if let Err(e) = download(&job) {
                errors.lock().unwrap().push(format!("{e:#}"));
            }
            task.inc();
        }));
    }
    for h in handles {
        let _ = h.join();
    }
    let errors = errors.lock().unwrap();
    if !errors.is_empty() {
        bail!("{} download(s) failed, first: {}", errors.len(), errors[0]);
    }
    Ok(())
}
