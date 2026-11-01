//! Yggdrasil accounts and authlib-injector. Passwords are used only for the login request.
use crate::{
    accounts::{Account, AccountKind, Result},
    minecraft::Downloader,
};
use reqwest::{
    Url,
    blocking::{Client as Http, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reference {
    pub root: String,
    pub login: String,
    pub profile: String,
}
#[derive(Clone)]
pub struct Server {
    pub root: String,
    pub name: String,
    pub metadata: Vec<u8>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Credential {
    pub access_token: String,
    pub client_token: String,
}
#[derive(Clone)]
pub struct Session {
    pub account: Account,
    pub credential: Credential,
    pub server: Server,
}
#[derive(Clone)]
pub struct Login {
    pub server: Server,
    pub username: String,
    pub profiles: Vec<Profile>,
    selected: Option<Profile>,
    credential: Credential,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResponseToken {
    access_token: String,
    client_token: String,
    #[serde(default)]
    available_profiles: Vec<Profile>,
    #[serde(default)]
    selected_profile: Option<Profile>,
}
pub trait Vault {
    fn read(&self, id: &str) -> Result<Credential>;
    fn write(&self, id: &str, credential: &Credential) -> Result<()>;
    fn remove(&self, id: &str) -> Result<()>;
}
pub struct SystemVault;
#[cfg(any(windows, target_os = "macos"))]
fn entry(id: &str) -> Result<keyring::Entry> {
    if !crate::accounts::valid_profile_id(id) {
        return Err("外置账号标识无效。".into());
    }
    keyring::Entry::new(crate::store::APP_IDENTIFIER, &format!("external/{id}"))
        .map_err(|_| "无法访问系统凭据库。".into())
}
impl Vault for SystemVault {
    fn read(&self, id: &str) -> Result<Credential> {
        #[cfg(any(windows, target_os = "macos"))]
        {
            let s = entry(id)?
                .get_password()
                .map_err(|_| "外置账号凭据不可用，请重新登录。")?;
            serde_json::from_str(&s).map_err(|_| "外置账号凭据损坏，请重新登录。".into())
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            let _ = id;
            Err("此平台的外置账号仅支持当前会话登录。".into())
        }
    }
    fn write(&self, id: &str, c: &Credential) -> Result<()> {
        #[cfg(any(windows, target_os = "macos"))]
        {
            entry(id)?
                .set_password(&serde_json::to_string(c).map_err(|_| "无法编码外置凭据。")?)
                .map_err(|_| "外置账号仅在本次会话有效，系统凭据库保存失败。".into())
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            let _ = (id, c);
            Err("此平台的外置账号仅支持当前会话登录。".into())
        }
    }
    fn remove(&self, id: &str) -> Result<()> {
        #[cfg(any(windows, target_os = "macos"))]
        {
            match entry(id)?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(_) => Err("无法删除外置认证凭据。".into()),
            }
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            let _ = id;
            Ok(())
        }
    }
}
fn url(value: &str) -> Result<Url> {
    if value.len() > 2048 || value.chars().any(char::is_control) {
        return Err("认证地址无效。".into());
    }
    let mut u = Url::parse(value).map_err(|_| "请输入完整的 HTTPS 认证地址。")?;
    #[cfg(test)]
    let allowed =
        u.scheme() == "https" || u.scheme() == "http" && u.host_str() == Some("127.0.0.1");
    #[cfg(not(test))]
    let allowed = u.scheme() == "https";
    if !allowed
        || u.host_str().is_none()
        || !u.username().is_empty()
        || u.password().is_some()
        || u.query().is_some()
        || u.fragment().is_some()
    {
        return Err("认证地址必须使用 HTTPS，不能包含账号密码、查询串或片段。".into());
    }
    if !u.path().ends_with('/') {
        let p = format!("{}/", u.path());
        u.set_path(&p);
    }
    Ok(u)
}
fn account_id(reference: &Reference) -> String {
    let mut h = Sha256::new();
    for s in [&reference.root, &reference.login, &reference.profile] {
        h.update(s.as_bytes());
        h.update([0]);
    }
    format!("{:x}", h.finalize())[..32].into()
}
pub fn valid_profile_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name.encode_utf16().count() <= 16
        && !name.chars().any(char::is_control)
}
pub fn validate_account(account: &Account) -> Result<()> {
    let r = account.external.as_ref().ok_or("外置账号缺少认证来源。")?;
    if account.kind != AccountKind::External
        || !valid_profile_name(&account.name)
        || url(&r.root)?.as_str() != r.root
        || r.login.is_empty()
        || r.login.len() > 256
        || r.login.chars().any(char::is_control)
        || !crate::accounts::valid_profile_id(&r.profile)
        || account_id(r) != account.id
    {
        return Err("外置账号来源或身份标识无效。".into());
    }
    Ok(())
}
fn check(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err("登录已取消。".into())
    } else {
        Ok(())
    }
}
fn bytes(response: Response, limit: u64) -> Result<Vec<u8>> {
    let mut b = Vec::new();
    response
        .take(limit + 1)
        .read_to_end(&mut b)
        .map_err(|_| "无法读取外置认证响应。")?;
    if b.len() as u64 > limit {
        return Err("外置认证响应过大。".into());
    }
    Ok(b)
}
pub struct Client {
    http: Http,
}
impl Client {
    pub fn new() -> Result<Self> {
        Ok(Self {
            http: Http::builder()
                .timeout(Duration::from_secs(25))
                .connect_timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent(concat!("NSCL/", env!("CARGO_PKG_VERSION")))
                .build()
                .map_err(|_| "无法创建外置认证连接。")?,
        })
    }
    pub fn discover(&self, input: &str, cancel: &AtomicBool) -> Result<Server> {
        let input = input.trim();
        let input = if input.contains("://") {
            input.to_owned()
        } else {
            format!("https://{input}")
        };
        let mut root = url(&input)?;
        for _ in 0..6 {
            check(cancel)?;
            let response = self
                .http
                .get(root.clone())
                .send()
                .map_err(|_| "认证服务器连接失败，请检查地址和网络。")?;
            if response.status().is_redirection() {
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|s| s.to_str().ok())
                    .ok_or("认证服务重定向缺少地址。")?;
                root = url(root
                    .join(location)
                    .map_err(|_| "认证重定向无效。")?
                    .as_str())?;
                continue;
            }
            if !response.status().is_success() {
                return Err(format!(
                    "认证服务器返回 HTTP {}。",
                    response.status().as_u16()
                ));
            }
            if let Some(location) = response
                .headers()
                .get("X-Authlib-Injector-API-Location")
                .and_then(|s| s.to_str().ok())
            {
                let next = url(root
                    .join(location)
                    .map_err(|_| "认证服务 API 地址无效。")?
                    .as_str())?;
                if next != root {
                    root = next;
                    continue;
                }
            }
            let metadata = bytes(response, 65536)?;
            let value: Value = serde_json::from_slice(&metadata)
                .map_err(|_| "此地址未返回 Yggdrasil API 元数据。")?;
            if !value["meta"].is_object()
                || !value["signaturePublickey"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("-----BEGIN PUBLIC KEY-----") && s.len() < 32768)
            {
                return Err("认证服务缺少有效的签名公钥或元数据。".into());
            }
            let name = value["meta"]["serverName"]
                .as_str()
                .unwrap_or(root.host_str().unwrap())
                .chars()
                .filter(|c| !c.is_control())
                .take(128)
                .collect();
            return Ok(Server {
                root: root.to_string(),
                name,
                metadata,
            });
        }
        Err("认证服务跳转或 API 指示次数过多。".into())
    }
    fn post(
        &self,
        server: &Server,
        path: &str,
        body: &Value,
        cancel: &AtomicBool,
    ) -> Result<ResponseToken> {
        check(cancel)?;
        let root = url(&server.root)?;
        let response = self
            .http
            .post(root.join(path).map_err(|_| "认证路径无效。")?)
            .json(body)
            .send()
            .map_err(|_| "外置认证连接失败，请检查网络。")?;
        if !response.status().is_success() {
            return Err(format!(
                "外置认证失败（HTTP {}），请检查账号、密码、服务器状态或重新登录。",
                response.status().as_u16()
            ));
        }
        let mut result: ResponseToken = serde_json::from_slice(&bytes(response, 1024 * 1024)?)
            .map_err(|_| "外置认证返回数据不完整。")?;
        if result.access_token.is_empty()
            || result.access_token.len() > 8192
            || result.access_token.chars().any(char::is_control)
            || result.client_token.is_empty()
            || result.client_token.len() > 1024
            || result.available_profiles.len() > 100
        {
            return Err("外置认证令牌或角色列表无效。".into());
        }
        let strict = serde_json::from_slice::<Value>(&server.metadata)
            .ok()
            .and_then(|v| v["meta"]["feature.username_check"].as_bool())
            .unwrap_or(false);
        for p in result
            .available_profiles
            .iter_mut()
            .chain(result.selected_profile.iter_mut())
        {
            p.id.make_ascii_lowercase();
            if !crate::accounts::valid_profile_id(&p.id)
                || !valid_profile_name(&p.name)
                || strict && crate::minecraft::validate_player_name(&p.name).is_err()
            {
                return Err("外置认证角色 UUID 或名称无效。".into());
            }
        }
        check(cancel)?;
        Ok(result)
    }
    pub fn login(
        &self,
        server: &Server,
        username: &str,
        password: &str,
        cancel: &AtomicBool,
    ) -> Result<Login> {
        if username.trim().is_empty()
            || username.len() > 256
            || username.chars().any(char::is_control)
            || password.is_empty()
            || password.len() > 8192
        {
            return Err("请填写有效的外置账号和密码。".into());
        }
        let token = uuid::Uuid::new_v4().simple().to_string();
        let result=self.post(server,"authserver/authenticate",&json!({"username":username.trim(),"password":password,"clientToken":token,"requestUser":true,"agent":{"name":"Minecraft","version":1}}),cancel)?;
        if result.client_token != token {
            return Err("外置认证 clientToken 不匹配。".into());
        }
        if result.selected_profile.is_none() && result.available_profiles.is_empty() {
            return Err("该外置账号没有 Minecraft 角色，请先在认证站创建角色。".into());
        }
        Ok(Login {
            server: server.clone(),
            username: username.trim().into(),
            profiles: result.available_profiles,
            selected: result.selected_profile,
            credential: Credential {
                access_token: result.access_token,
                client_token: result.client_token,
            },
        })
    }
    pub fn select(
        &self,
        login: &Login,
        profile: Option<&Profile>,
        cancel: &AtomicBool,
    ) -> Result<Session> {
        let (profile, credential) = if let Some(p) = &login.selected {
            (p.clone(), login.credential.clone())
        } else {
            let p = profile
                .filter(|p| login.profiles.iter().any(|known| known.id == p.id))
                .ok_or("请选择此账号拥有的角色。")?;
            let result=self.post(&login.server,"authserver/refresh",&json!({"accessToken":login.credential.access_token,"clientToken":login.credential.client_token,"selectedProfile":p,"requestUser":true}),cancel)?;
            let selected = result
                .selected_profile
                .filter(|s| s.id == p.id)
                .ok_or("外置认证绑定了其他角色，已阻止登录。")?;
            if result.client_token != login.credential.client_token {
                return Err("外置认证 clientToken 不匹配。".into());
            }
            (
                selected,
                Credential {
                    access_token: result.access_token,
                    client_token: result.client_token,
                },
            )
        };
        let reference = Reference {
            root: login.server.root.clone(),
            login: login.username.clone(),
            profile: profile.id,
        };
        let account = Account {
            kind: AccountKind::External,
            id: account_id(&reference),
            name: profile.name,
            external: Some(reference),
            xuid: String::new(),
            client_id: String::new(),
        };
        validate_account(&account)?;
        Ok(Session {
            account,
            credential,
            server: login.server.clone(),
        })
    }
    pub fn resume(
        &self,
        account: &Account,
        cached: Option<Session>,
        vault: &dyn Vault,
        cancel: &AtomicBool,
    ) -> Result<Session> {
        validate_account(account)?;
        let cache_absent = cached.is_none();
        let reference = account.external.as_ref().unwrap();
        let credential = match cached {
            Some(s) if s.account.id == account.id => s.credential,
            _ => vault.read(&account.id)?,
        };
        let server = self.discover(&reference.root, cancel)?;
        if server.root != reference.root {
            return Err("认证 API 地址已改变，请重新确认服务器并登录。".into());
        }
        let result=self.post(&server,"authserver/refresh",&json!({"accessToken":credential.access_token,"clientToken":credential.client_token,"requestUser":true}),cancel)?;
        let profile = result
            .selected_profile
            .filter(|p| p.id == reference.profile)
            .ok_or("外置认证身份与所选角色不一致，请重新登录。")?;
        if result.client_token != credential.client_token {
            return Err("外置认证 clientToken 不匹配。".into());
        }
        let mut account = account.clone();
        account.name = profile.name;
        let credential = Credential {
            access_token: result.access_token,
            client_token: result.client_token,
        };
        if vault.write(&account.id, &credential).is_err() && cache_absent {
            return Err("系统凭据库无法更新外置账号令牌，请重新登录以使用会话模式。".into());
        }
        Ok(Session {
            account,
            credential,
            server,
        })
    }
}
impl Login {
    pub fn selected(&self) -> bool {
        self.selected.is_some()
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Agent {
    build_number: u64,
    download_url: String,
    checksums: Checksums,
}
#[derive(Clone, Serialize, Deserialize)]
struct Checksums {
    sha256: String,
}
#[derive(Serialize, Deserialize)]
struct AgentCache {
    #[serde(flatten)]
    agent: Agent,
    #[serde(default)]
    checked_at: u64,
    #[serde(default)]
    retry_after: u64,
}
const AGENT_CHECK_INTERVAL: u64 = 24 * 60 * 60;
const AGENT_RETRY_INTERVAL: u64 = 60 * 60;
#[cfg(test)]
#[path = "external_auth_tests.rs"]
mod tests;
pub fn install_agent(directory: &Path, d: &Downloader) -> Result<PathBuf> {
    install_agent_from(
        directory,
        d,
        "https://authlib-injector.yushi.moe/artifact/latest.json",
    )
}
fn install_agent_from(directory: &Path, d: &Downloader, manifest: &str) -> Result<PathBuf> {
    install_agent_at(directory, d, manifest, crate::accounts::now())
}
fn install_agent_at(directory: &Path, d: &Downloader, manifest: &str, now: u64) -> Result<PathBuf> {
    d.check_cancelled()?;
    let cache = directory.join("authlib-injector");
    for p in cache.ancestors() {
        crate::instances::reject_link(p)?;
    }
    let record = cache.join("artifact.json");
    crate::instances::reject_link(&record)?;
    let stored: Option<AgentCache> = std::fs::metadata(&record)
        .ok()
        .filter(|m| m.len() < 16384)
        .and_then(|_| std::fs::read(&record).ok())
        .and_then(|b| serde_json::from_slice(&b).ok());
    let previous = stored.as_ref().and_then(|c| {
        let path = agent_path(&cache, &c.agent).ok()?;
        verified_agent(&path, &c.agent).then_some(path)
    });
    let fresh = stored.as_ref().is_some_and(|c| {
        c.checked_at > 0 && now >= c.checked_at && now - c.checked_at < AGENT_CHECK_INTERVAL
    });
    let retry_pending = stored
        .as_ref()
        .is_some_and(|c| c.retry_after > now && c.retry_after - now <= AGENT_RETRY_INTERVAL);
    if let Some(path) = &previous
        && (fresh || retry_pending)
    {
        d.check_cancelled()?;
        return Ok(path.clone());
    }
    let updated = (|| {
        let agent = if fresh {
            stored.as_ref().unwrap().agent.clone()
        } else {
            d.phase("检查 authlib-injector 更新", 1);
            d.json::<Agent>(manifest)?
        };
        if previous.is_some()
            && let Some(old) = &stored
            && (agent.build_number < old.agent.build_number
                || agent.build_number == old.agent.build_number
                    && !agent
                        .checksums
                        .sha256
                        .eq_ignore_ascii_case(&old.agent.checksums.sha256))
        {
            return Err("authlib-injector 更新版本信息不一致。".into());
        }
        let path = fetch_agent(&cache, &agent, d)?;
        d.check_cancelled()?;
        let checked_at = if fresh {
            stored.as_ref().unwrap().checked_at
        } else {
            now
        };
        crate::store::write_json(
            &record,
            &AgentCache {
                agent,
                checked_at,
                retry_after: 0,
            },
        )?;
        Ok(path)
    })();
    match updated {
        Ok(path) => Ok(path),
        Err(error) => {
            // Cancellation must stop launch, including when a verified older JAR exists.
            d.check_cancelled()?;
            if let (Some(path), Some(mut old)) = (previous, stored) {
                old.retry_after = now.saturating_add(AGENT_RETRY_INTERVAL);
                let _ = crate::store::write_json(&record, &old);
                d.phase("更新检查未完成，继续使用已校验的 authlib-injector", 1);
                Ok(path)
            } else {
                Err(error)
            }
        }
    }
}
fn agent_path(cache: &Path, agent: &Agent) -> Result<PathBuf> {
    let hash = &agent.checksums.sha256;
    let download =
        Url::parse(&agent.download_url).map_err(|_| "authlib-injector 下载地址无效。")?;
    let allowed =
        download.scheme() == "https" && download.host_str() == Some("authlib-injector.yushi.moe");
    #[cfg(test)]
    let allowed =
        allowed || download.scheme() == "http" && download.host_str() == Some("127.0.0.1");
    if hash.len() != 64
        || !hash.bytes().all(|b| b.is_ascii_hexdigit())
        || !allowed
        || !download.username().is_empty()
        || download.password().is_some()
    {
        return Err("authlib-injector 官方下载元数据无效。".into());
    }
    let path = cache.join(format!(
        "{}-{}.jar",
        agent.build_number,
        hash.to_ascii_lowercase()
    ));
    crate::instances::reject_link(&path)?;
    Ok(path)
}
fn verified_agent(path: &Path, agent: &Agent) -> bool {
    std::fs::metadata(path)
        .ok()
        .filter(|m| m.len() <= 16 * 1024 * 1024)
        .and_then(|_| std::fs::read(path).ok())
        .is_some_and(|b| {
            format!("{:x}", Sha256::digest(&b)).eq_ignore_ascii_case(&agent.checksums.sha256)
        })
}
fn fetch_agent(cache: &Path, agent: &Agent, d: &Downloader) -> Result<PathBuf> {
    let path = agent_path(cache, agent)?;
    if verified_agent(&path, agent) {
        return Ok(path);
    }
    d.phase("下载并验证 authlib-injector", 1);
    d.fetch(&agent.download_url, |b| {
        if b.len() > 16 * 1024 * 1024
            || !format!("{:x}", Sha256::digest(b)).eq_ignore_ascii_case(&agent.checksums.sha256)
        {
            return Err("authlib-injector SHA-256 校验失败。".into());
        }
        std::fs::create_dir_all(cache).map_err(|e| e.to_string())?;
        let temp = cache.join(format!("agent-{}.tmp", uuid::Uuid::new_v4().simple()));
        let result = (|| {
            std::fs::write(&temp, b).map_err(|e| e.to_string())?;
            d.check_cancelled()?;
            crate::minecraft::replace_file(&temp, &path)
        })();
        let _ = std::fs::remove_file(temp);
        result
    })?;
    Ok(path)
}
