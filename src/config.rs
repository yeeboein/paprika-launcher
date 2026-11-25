//! Settings and accounts, stored as JSON in the Paprika folder.

use crate::auth::Account;
use crate::paths;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// Microsoft (Azure) app client id baked in at build time, if any.
pub const BUILTIN_CLIENT_ID: Option<&str> = option_env!("PAPRIKA_MSA_CLIENT_ID");

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub memory_mb: u32,
    pub java_path: String,
    pub jvm_args: String,
    pub msa_client_id: String,
    pub width: u32,
    pub height: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            memory_mb: 4096,
            java_path: String::new(),
            jvm_args: String::new(),
            msa_client_id: BUILTIN_CLIENT_ID.unwrap_or_default().to_string(),
            width: 1280,
            height: 720,
        }
    }
}

impl Settings {
    pub fn client_id(&self) -> Option<String> {
        let id = self.msa_client_id.trim();
        if !id.is_empty() {
            Some(id.to_string())
        } else {
            BUILTIN_CLIENT_ID
                .filter(|s| !s.is_empty())
                .map(String::from)
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Accounts {
    pub accounts: Vec<Account>,
    pub active: Option<String>,
}

impl Accounts {
    pub fn active(&self) -> Option<&Account> {
        let id = self.active.as_ref()?;
        self.accounts.iter().find(|a| &a.uuid == id)
    }
    pub fn upsert(&mut self, acc: Account) {
        self.active = Some(acc.uuid.clone());
        if let Some(a) = self.accounts.iter_mut().find(|a| a.uuid == acc.uuid) {
            *a = acc;
        } else {
            self.accounts.push(acc);
        }
    }
}

fn load<T: DeserializeOwned + Default>(name: &str) -> T {
    std::fs::read_to_string(paths::root().join(name))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save<T: Serialize>(name: &str, v: &T) {
    let _ = std::fs::create_dir_all(paths::root());
    if let Ok(s) = serde_json::to_string_pretty(v) {
        let path = paths::root().join(name);
        let _ = std::fs::write(&path, s);
        #[cfg(unix)]
        if name == "accounts.json" {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
    }
}

pub fn load_settings() -> Settings {
    load("settings.json")
}
pub fn save_settings(s: &Settings) {
    save("settings.json", s)
}
pub fn load_accounts() -> Accounts {
    load("accounts.json")
}
pub fn save_accounts(a: &Accounts) {
    save("accounts.json", a)
}
