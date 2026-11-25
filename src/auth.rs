//! Accounts: offline players and Microsoft sign-in (device code flow).

use crate::net::{self, describe};
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum AccountKind {
    Offline,
    Microsoft,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Account {
    pub kind: AccountKind,
    pub username: String,
    /// UUID without dashes.
    pub uuid: String,
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub expires_at: u64,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Same UUID vanilla servers use for an offline player name.
pub fn offline(username: &str) -> Account {
    let mut b = md5::compute(format!("OfflinePlayer:{username}")).0;
    b[6] = (b[6] & 0x0f) | 0x30;
    b[8] = (b[8] & 0x3f) | 0x80;
    let uuid: String = b.iter().map(|x| format!("{x:02x}")).collect();
    Account {
        kind: AccountKind::Offline,
        username: username.to_string(),
        uuid,
        access_token: "0".into(),
        refresh_token: String::new(),
        expires_at: 0,
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct DeviceCode {
    pub user_code: String,
    pub device_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

const MS_DEVICE: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const MS_TOKEN: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const SCOPE: &str = "XboxLive.signin offline_access";

#[derive(Deserialize)]
struct MsToken {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
}

pub fn start_device_code(client_id: &str) -> Result<DeviceCode> {
    let resp = net::agent()
        .post(MS_DEVICE)
        .send_form(&[("client_id", client_id), ("scope", SCOPE)])
        .map_err(describe)?;
    Ok(resp.into_json()?)
}

/// Wait for the player to finish signing in, then log into Minecraft.
pub fn finish_device_code(
    client_id: &str,
    dc: &DeviceCode,
    cancel: &AtomicBool,
) -> Result<Account> {
    let mut interval = dc.interval.max(1);
    let deadline = now() + dc.expires_in;
    loop {
        for _ in 0..interval * 10 {
            if cancel.load(Ordering::SeqCst) {
                bail!("sign-in cancelled");
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if now() > deadline {
            bail!("the sign-in code expired, try again");
        }
        let res = net::agent().post(MS_TOKEN).send_form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", client_id),
            ("device_code", &dc.device_code),
        ]);
        match res {
            Ok(r) => {
                let t: MsToken = r.into_json()?;
                return minecraft_login(&t.access_token, &t.refresh_token);
            }
            Err(ureq::Error::Status(_, r)) => {
                let v: Value = r.into_json().unwrap_or_default();
                match v["error"].as_str() {
                    Some("authorization_pending") => continue,
                    Some("slow_down") => interval += 5,
                    _ => bail!(
                        "Microsoft sign-in failed: {}",
                        v["error_description"].as_str().unwrap_or("unknown error")
                    ),
                }
            }
            Err(e) => return Err(describe(e)),
        }
    }
}

/// Refresh an expired Microsoft account. Offline accounts pass through.
pub fn ensure_fresh(acc: &Account, client_id: Option<&str>) -> Result<Account> {
    if acc.kind == AccountKind::Offline || now() + 300 < acc.expires_at {
        return Ok(acc.clone());
    }
    let client_id = client_id.context("Microsoft client ID is missing (Settings)")?;
    let resp = net::agent()
        .post(MS_TOKEN)
        .send_form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("refresh_token", &acc.refresh_token),
            ("scope", SCOPE),
        ])
        .map_err(|e| anyhow!("{} - please sign in again", describe(e)))?;
    let t: MsToken = resp.into_json()?;
    minecraft_login(&t.access_token, &t.refresh_token)
}

fn post_json(url: &str, body: Value) -> Result<Value> {
    match net::agent()
        .post(url)
        .set("Accept", "application/json")
        .send_json(body)
    {
        Ok(r) => Ok(r.into_json()?),
        Err(ureq::Error::Status(code, r)) => {
            let v: Value = r.into_json().unwrap_or_default();
            if let Some(x) = v["XErr"].as_u64() {
                bail!(match x {
                    2148916233 => "this Microsoft account has no Xbox profile - sign in once at minecraft.net first".to_string(),
                    2148916235 => "Xbox Live isn't available in your country".to_string(),
                    2148916238 => "child account: add it to a Microsoft family first".to_string(),
                    other => format!("Xbox error {other}"),
                });
            }
            bail!("HTTP {code} from {url}: {v}")
        }
        Err(e) => Err(describe(e)),
    }
}

fn minecraft_login(ms_access: &str, refresh: &str) -> Result<Account> {
    let xbl = post_json(
        "https://user.auth.xboxlive.com/user/authenticate",
        json!({
            "Properties": {"AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com", "RpsTicket": format!("d={ms_access}")},
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        }),
    )?;
    let xbl_token = xbl["Token"].as_str().context("Xbox token")?;
    let xsts = post_json(
        "https://xsts.auth.xboxlive.com/xsts/authorize",
        json!({
            "Properties": {"SandboxId": "RETAIL", "UserTokens": [xbl_token]},
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        }),
    )?;
    let xsts_token = xsts["Token"].as_str().context("XSTS token")?;
    let uhs = xsts["DisplayClaims"]["xui"][0]["uhs"]
        .as_str()
        .context("user hash")?;
    let mc = post_json(
        "https://api.minecraftservices.com/authentication/login_with_xbox",
        json!({"identityToken": format!("XBL3.0 x={uhs};{xsts_token}")}),
    )?;
    let mc_token = mc["access_token"]
        .as_str()
        .context("Minecraft token")?
        .to_string();
    let expires_in = mc["expires_in"].as_u64().unwrap_or(86400);
    let profile: Value = match net::agent()
        .get("https://api.minecraftservices.com/minecraft/profile")
        .set("Authorization", &format!("Bearer {mc_token}"))
        .call()
    {
        Ok(r) => r.into_json()?,
        Err(ureq::Error::Status(404, _)) => {
            bail!("this account doesn't own Minecraft: Java Edition")
        }
        Err(e) => return Err(describe(e)),
    };
    Ok(Account {
        kind: AccountKind::Microsoft,
        username: profile["name"].as_str().unwrap_or("Player").to_string(),
        uuid: profile["id"].as_str().context("profile id")?.to_string(),
        access_token: mc_token,
        refresh_token: refresh.to_string(),
        expires_at: now() + expires_in,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn offline_uuid_matches_vanilla() {
        // Java: UUID.nameUUIDFromBytes("OfflinePlayer:Notch".getBytes(UTF_8))
        assert_eq!(
            super::offline("Notch").uuid,
            "b50ad385829d3141a2167e7d7539ba7f"
        );
    }
}
