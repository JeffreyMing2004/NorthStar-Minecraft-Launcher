//! Microsoft browser/device authorization and Minecraft identity. Tokens never enter diagnostics.
mod browser;
pub use browser::{
    BrowserAuthorization, BrowserPrompt, DEFAULT_REDIRECT_URI, validate_redirect_uri,
};
use reqwest::blocking::{Client, RequestBuilder, Response};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const DEFAULT_CLIENT_ID: &str = "0c342b6c-38ee-414d-8f83-8df917817e2b";
const SCOPE: &str = "XboxLive.signin offline_access";
pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Copy, Default, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    #[default]
    Microsoft,
    Offline,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Account {
    #[serde(default)]
    pub kind: AccountKind,
    pub id: String,
    pub name: String,
    pub xuid: String,
    pub client_id: String,
}

impl Account {
    pub fn offline(name: &str) -> Result<Self> {
        let name = name.trim();
        crate::minecraft::validate_player_name(name)?;
        Ok(Self {
            kind: AccountKind::Offline,
            id: crate::minecraft::offline_uuid(name),
            name: name.into(),
            xuid: String::new(),
            client_id: String::new(),
        })
    }
    pub fn is_online(&self) -> bool {
        self.kind == AccountKind::Microsoft
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AccountList {
    pub accounts: Vec<Account>,
    pub selected: Option<String>,
}

impl AccountList {
    pub fn path() -> PathBuf {
        crate::store::settings_path().with_file_name("nscl-accounts.json")
    }
    pub fn load() -> Result<Self> {
        let path = Self::path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let data: Self =
            serde_json::from_slice(&std::fs::read(path).map_err(|_| "无法读取账号列表。")?)
                .map_err(|_| "账号列表损坏，请备份 nscl-accounts.json 后重新登录。")?;
        data.validate()?;
        Ok(data)
    }
    pub fn save(&self) -> Result<()> {
        self.validate()?;
        crate::store::write_json(&Self::path(), self)
    }
    pub fn validate(&self) -> Result<()> {
        let mut ids = std::collections::BTreeSet::new();
        for account in &self.accounts {
            if !valid_profile_id(&account.id)
                || crate::minecraft::validate_player_name(&account.name).is_err()
                || !ids.insert(&account.id)
                || (!account.is_online() && Account::offline(&account.name)? != *account)
            {
                return Err("账号列表包含无效或重复身份。".into());
            }
        }
        Ok(())
    }
    pub fn add_offline(&mut self, name: &str) -> Result<()> {
        let account = Account::offline(name)?;
        if self
            .accounts
            .iter()
            .any(|a| !a.is_online() && a.name.eq_ignore_ascii_case(&account.name))
        {
            return Err("此离线账号已存在，请直接选择使用。".into());
        }
        self.upsert(account);
        Ok(())
    }
    pub fn current(&self) -> Option<&Account> {
        self.accounts
            .iter()
            .find(|a| Some(&a.id) == self.selected.as_ref())
    }
    pub fn upsert(&mut self, account: Account) {
        self.selected = Some(account.id.clone());
        self.accounts.retain(|a| a.id != account.id);
        self.accounts.push(account);
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct RefreshCredential {
    pub client_id: String,
    pub refresh_token: String,
}

pub trait CredentialStore: Send + Sync {
    fn read(&self, id: &str) -> Result<RefreshCredential>;
    fn write(&self, id: &str, credential: &RefreshCredential) -> Result<()>;
    fn remove(&self, id: &str) -> Result<()>;
}

pub struct SystemVault;
#[cfg(any(windows, target_os = "macos"))]
fn credential_entry(id: &str) -> Result<keyring::Entry> {
    if !valid_profile_id(id) {
        return Err("无效账号标识。".into());
    }
    keyring::Entry::new(crate::store::APP_IDENTIFIER, &format!("microsoft/{id}"))
        .map_err(|_| "无法访问系统凭据库。".into())
}
#[cfg(any(windows, target_os = "macos"))]
impl CredentialStore for SystemVault {
    fn read(&self, id: &str) -> Result<RefreshCredential> {
        let secret = credential_entry(id)?
            .get_password()
            .map_err(|_| "没有可用的登录凭据，请重新登录 Microsoft 账号。")?;
        serde_json::from_str(&secret).map_err(|_| "登录凭据损坏，请重新登录。".into())
    }
    fn write(&self, id: &str, credential: &RefreshCredential) -> Result<()> {
        let secret = serde_json::to_string(credential).map_err(|_| "无法编码登录凭据。")?;
        credential_entry(id)?
            .set_password(&secret)
            .map_err(|_| "系统凭据库写入失败，本次登录仅在当前会话有效。".into())
    }
    fn remove(&self, id: &str) -> Result<()> {
        match credential_entry(id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err("无法移除系统登录凭据，请稍后重试。".into()),
        }
    }
}
#[cfg(not(any(windows, target_os = "macos")))]
impl CredentialStore for SystemVault {
    fn read(&self, _: &str) -> Result<RefreshCredential> {
        Err("此平台仅支持会话登录，请重新登录。".into())
    }
    fn write(&self, _: &str, _: &RefreshCredential) -> Result<()> {
        Err("此平台尚未接入系统凭据库，本次登录仅在当前会话有效。".into())
    }
    fn remove(&self, _: &str) -> Result<()> {
        Ok(())
    }
}

#[derive(Clone)]
pub struct Session {
    pub account: Account,
    pub credential: RefreshCredential,
    pub access_token: String,
    pub expires_at: u64,
}

#[derive(Deserialize)]
pub struct DeviceCode {
    device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    #[serde(default = "default_interval")]
    interval: u64,
}
fn default_interval() -> u64 {
    5
}

#[derive(Deserialize)]
struct MicrosoftToken {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
}

#[derive(Clone)]
struct Endpoints {
    microsoft: String,
    xbox: String,
    xsts: String,
    minecraft: String,
}
impl Default for Endpoints {
    fn default() -> Self {
        Self {
            microsoft: "https://login.microsoftonline.com/consumers/oauth2/v2.0".into(),
            xbox: "https://user.auth.xboxlive.com/user/authenticate".into(),
            xsts: "https://xsts.auth.xboxlive.com/xsts/authorize".into(),
            minecraft: "https://api.minecraftservices.com".into(),
        }
    }
}

pub struct AuthClient {
    client: Client,
    endpoints: Endpoints,
}
impl AuthClient {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(25))
                .connect_timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent(concat!("NSCL/", env!("CARGO_PKG_VERSION")))
                .build()
                .map_err(|_| "无法创建登录连接。")?,
            endpoints: Endpoints::default(),
        })
    }
    pub fn device_code(&self, client_id: &str) -> Result<DeviceCode> {
        validate_client_id(client_id)?;
        let code: DeviceCode = decode(
            self.client
                .post(format!("{}/devicecode", self.endpoints.microsoft))
                .form(&[("client_id", client_id), ("scope", SCOPE)]),
            "Microsoft 设备授权",
        )?;
        if code.device_code.is_empty()
            || code.user_code.is_empty()
            || code.expires_in == 0
            || code.expires_in > 3600
        {
            return Err("Microsoft 返回了无效设备代码。".into());
        }
        let uri = reqwest::Url::parse(&code.verification_uri)
            .map_err(|_| "无效的 Microsoft 授权网址。")?;
        if uri.scheme() != "https"
            || !matches!(
                uri.host_str(),
                Some("microsoft.com" | "www.microsoft.com" | "login.microsoftonline.com")
            )
        {
            return Err("Microsoft 返回了不受信任的授权网址。".into());
        }
        Ok(code)
    }
    pub fn login(
        &self,
        client_id: &str,
        cancel: &AtomicBool,
        show: impl FnOnce(&DeviceCode),
        stage: impl Fn(&str),
    ) -> Result<Session> {
        check_cancel(cancel)?;
        let code = self.device_code(client_id)?;
        let deadline = Instant::now() + Duration::from_secs(code.expires_in);
        let mut interval = code.interval.clamp(1, 60);
        show(&code);
        loop {
            wait(interval, deadline, cancel)?;
            let response = self
                .client
                .post(format!("{}/token", self.endpoints.microsoft))
                .form(&[
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("client_id", client_id),
                    ("device_code", &code.device_code),
                ])
                .send();
            let response = match response {
                Ok(response) => response,
                Err(error) if error.is_timeout() => {
                    interval = (interval * 2).min(60);
                    continue;
                }
                Err(_) => return Err("Microsoft 登录连接失败，请检查网络后重试。".into()),
            };
            let status = response.status();
            let body = body(response)?;
            match body.get("error").and_then(Value::as_str) {
                Some("authorization_pending") => continue,
                Some("slow_down") => {
                    interval = (interval + 5).min(120);
                    continue;
                }
                Some(_) => return Err(oauth_response_error(&body)),
                None if !status.is_success() => {
                    return Err(format!("Microsoft 授权失败（HTTP {status}）。"));
                }
                _ => {}
            }
            check_cancel(cancel)?;
            let token: MicrosoftToken =
                serde_json::from_value(body).map_err(|_| "Microsoft 登录响应不完整。")?;
            if token.refresh_token.is_empty() {
                return Err(
                    "Microsoft 未授予续期权限，请检查公共客户端及 offline_access 配置。".into(),
                );
            }
            return self.exchange(client_id, token, cancel, &stage);
        }
    }
    pub fn resume(
        &self,
        account: &Account,
        cached: Option<Session>,
        vault: &dyn CredentialStore,
        cancel: &AtomicBool,
    ) -> Result<Session> {
        if !account.is_online() {
            return Err("离线账号不使用 Microsoft 登录凭据。".into());
        }
        check_cancel(cancel)?;
        if let Some(session) = cached.as_ref().filter(|s| s.expires_at > now() + 120) {
            match self.profile(&session.access_token) {
                Ok(profile) => {
                    if profile.0 != account.id {
                        return Err("Minecraft 身份与所选账号不一致，请重新登录。".into());
                    }
                    let mut session = session.clone();
                    session.account.name = profile.1;
                    return Ok(session);
                }
                Err(error) if error.contains("HTTP 401") => {}
                Err(error) => return Err(error),
            }
        }
        let cached_was_absent = cached.is_none();
        let credential = match cached {
            Some(s) => s.credential,
            None => vault.read(&account.id)?,
        };
        validate_client_id(&credential.client_id)?;
        let mut token: MicrosoftToken = decode(
            self.client
                .post(format!("{}/token", self.endpoints.microsoft))
                .form(&[
                    ("client_id", credential.client_id.as_str()),
                    ("grant_type", "refresh_token"),
                    ("refresh_token", credential.refresh_token.as_str()),
                    ("scope", SCOPE),
                ]),
            "Microsoft 登录续期",
        )?;
        if token.refresh_token.is_empty() {
            token.refresh_token = credential.refresh_token;
        }
        // Store rotated refresh tokens even if a later Xbox/profile request fails.
        let storage_result = vault.write(
            &account.id,
            &RefreshCredential {
                client_id: credential.client_id.clone(),
                refresh_token: token.refresh_token.clone(),
            },
        );
        // A session-only login can still renew in memory when native storage is unavailable.
        if cached_was_absent && storage_result.is_err() {
            return Err("系统凭据库无法更新续期凭据，请重新登录以使用会话模式。".into());
        }
        let session = self.exchange(&credential.client_id, token, cancel, &|_| {})?;
        if session.account.id != account.id {
            return Err("续期后的 Minecraft 身份不一致，请重新登录。".into());
        }
        Ok(session)
    }
    fn exchange(
        &self,
        client_id: &str,
        token: MicrosoftToken,
        cancel: &AtomicBool,
        stage: &impl Fn(&str),
    ) -> Result<Session> {
        check_cancel(cancel)?;
        stage("验证 Xbox Live 身份");
        let xbox: Value = decode(self.client.post(&self.endpoints.xbox).header("x-xbl-contract-version", "1").json(&json!({
            "Properties":{"AuthMethod":"RPS","SiteName":"user.auth.xboxlive.com","RpsTicket":format!("d={}", token.access_token)},
            "RelyingParty":"http://auth.xboxlive.com","TokenType":"JWT"
        })), "Xbox Live")?;
        let user_token = string(&xbox, "/Token")?;
        let hash = string(&xbox, "/DisplayClaims/xui/0/uhs")?;
        check_cancel(cancel)?;
        stage("获取 Minecraft 服务授权");
        let xsts: Value = decode(
            self.client
                .post(&self.endpoints.xsts)
                .header("x-xbl-contract-version", "1")
                .json(&json!({
                    "Properties":{"SandboxId":"RETAIL","UserTokens":[user_token]},
                    "RelyingParty":"rp://api.minecraftservices.com/","TokenType":"JWT"
                })),
            "Xbox XSTS",
        )?;
        if string(&xsts, "/DisplayClaims/xui/0/uhs")? != hash {
            return Err("Xbox 返回的用户身份不一致。".into());
        }
        let xuid = xsts
            .pointer("/DisplayClaims/xui/0/xid")
            .or_else(|| xbox.pointer("/DisplayClaims/xui/0/xid"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let identity_token = format!("XBL3.0 x={hash};{}", string(&xsts, "/Token")?);
        let minecraft = compatible_request(
            self.client
                .post(format!("{}/launcher/login", self.endpoints.minecraft))
                .json(&json!({"xtoken":identity_token, "platform":"PC_LAUNCHER"})),
            self.client
                .post(format!(
                    "{}/authentication/login_with_xbox",
                    self.endpoints.minecraft
                ))
                .json(&json!({"identityToken":identity_token, "ensureLegacyEnabled":true})),
            "Minecraft 登录（请确认 Client ID 的 Minecraft API 权限）",
        )?;
        let access_token = string(&minecraft, "/access_token")?.to_owned();
        let lifetime = minecraft
            .get("expires_in")
            .and_then(Value::as_u64)
            .filter(|&v| v > 0)
            .ok_or("Minecraft 未返回令牌有效期。")?;
        check_cancel(cancel)?;
        stage("验证 Java 版授权并获取玩家档案");
        let (id, name) = self.profile(&access_token)?;
        check_cancel(cancel)?;
        Ok(Session {
            account: Account {
                kind: AccountKind::Microsoft,
                id,
                name,
                xuid,
                client_id: client_id.into(),
            },
            credential: RefreshCredential {
                client_id: client_id.into(),
                refresh_token: token.refresh_token,
            },
            access_token,
            expires_at: now().saturating_add(lifetime),
        })
    }
    fn profile(&self, token: &str) -> Result<(String, String)> {
        let entitlements = compatible_request(
            self.client
                .get(format!("{}/entitlements/license", self.endpoints.minecraft))
                .query(&[("requestId", uuid::Uuid::new_v4().to_string())])
                .bearer_auth(token),
            self.client
                .get(format!("{}/entitlements/mcstore", self.endpoints.minecraft))
                .bearer_auth(token),
            "Minecraft Java 授权验证",
        )?;
        let owns = entitlements
            .get("items")
            .and_then(Value::as_array)
            .is_some_and(|items| {
                items.iter().any(|item| {
                    matches!(
                        item.get("name").and_then(Value::as_str),
                        Some("game_minecraft" | "product_minecraft")
                    )
                })
            });
        if !owns {
            return Err("此 Microsoft 账号没有有效的 Minecraft Java Edition 授权，请检查购买账号或 Game Pass 订阅。".into());
        }
        let profile: Value = decode(
            self.client
                .get(format!("{}/minecraft/profile", self.endpoints.minecraft))
                .bearer_auth(token),
            "Minecraft 玩家档案（首次使用请先创建角色）",
        )?;
        let id = string(&profile, "/id")?.to_owned();
        let name = string(&profile, "/name")?.to_owned();
        if !valid_profile_id(&id) || crate::minecraft::validate_player_name(&name).is_err() {
            return Err("Minecraft 玩家档案无效。".into());
        }
        Ok((id, name))
    }
}

fn string<'a>(body: &'a Value, pointer: &str) -> Result<&'a str> {
    body.pointer(pointer)
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "认证服务响应缺少必要字段。".into())
}
fn body(response: Response) -> Result<Value> {
    use std::io::Read;
    let mut bytes = Vec::new();
    response
        .take(1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|_| "无法读取认证响应。")?;
    serde_json::from_slice(&bytes).map_err(|_| "认证服务返回了无法识别的响应。".into())
}
fn decode<T: DeserializeOwned>(request: RequestBuilder, stage: &str) -> Result<T> {
    let response = request
        .send()
        .map_err(|_| format!("{stage}连接失败，请检查网络。"))?;
    decode_response(response, stage)
}
fn compatible_request(
    current: RequestBuilder,
    legacy: RequestBuilder,
    stage: &str,
) -> Result<Value> {
    let response = current
        .send()
        .map_err(|_| format!("{stage}连接失败，请检查网络。"))?;
    if matches!(response.status().as_u16(), 404 | 405) {
        decode(legacy, stage)
    } else {
        decode_response(response, stage)
    }
}
fn decode_response<T: DeserializeOwned>(response: Response, stage: &str) -> Result<T> {
    let status = response.status();
    let value = body(response)?;
    if !status.is_success() {
        if let Some(
            error @ ("invalid_grant"
            | "invalid_client"
            | "unauthorized_client"
            | "invalid_scope"
            | "interaction_required"
            | "access_denied"
            | "expired_token"
            | "authorization_declined"),
        ) = value.get("error").and_then(Value::as_str)
        {
            let _ = error;
            return Err(oauth_response_error(&value));
        }
        if let Some(code) = value.get("XErr").and_then(Value::as_u64) {
            return Err(xbox_error(code));
        }
        return Err(format!("{stage}失败（HTTP {status}）。"));
    }
    serde_json::from_value(value).map_err(|_| format!("{stage}响应不完整。"))
}
fn oauth_error(error: &str) -> String {
    match error {
        "authorization_declined" | "access_denied" => "已拒绝 Microsoft 登录授权。",
        "expired_token" => "登录代码已过期，请重新登录。",
        "invalid_client" | "unauthorized_client" => {
            "Microsoft 拒绝该应用的授权请求。请检查 Client ID 和个人账号支持；配置桌面回调后请使用浏览器登录。设备代码登录需要额外启用对应流程。"
        }
        "invalid_grant" | "interaction_required" => "登录凭据已失效，请重新登录 Microsoft 账号。",
        "invalid_scope" => "应用未允许 Xbox Live / offline_access 权限，请检查应用注册。",
        _ => "Microsoft 认证失败，请检查应用权限或重新登录。",
    }
    .into()
}
fn oauth_response_error(value: &Value) -> String {
    let error = value.get("error").and_then(Value::as_str).unwrap_or("");
    let code = value
        .get("error_codes")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(Value::as_u64)
        .or_else(|| {
            let description = value.get("error_description").and_then(Value::as_str)?;
            let digits: String = description
                .split("AADSTS")
                .nth(1)?
                .chars()
                .take_while(char::is_ascii_digit)
                .take(10)
                .collect();
            digits.parse().ok()
        });
    let message = match code {
        Some(50011) => "登录回调 URI 与应用注册不匹配，请将设置中的完整地址登记为移动和桌面应用程序回调。".into(),
        Some(700016) => "Microsoft 找不到该应用，请检查 Client ID，以及应用是否支持个人 Microsoft 账号。".into(),
        Some(7000218 | 9002326) => "应用要求 Web/SPA 客户端认证。请将本地回调登记在移动和桌面应用程序平台，NSCL 使用 PKCE，不需要 Client Secret。".into(),
        Some(50020 | 50194) => "该应用或账号不适用于个人 Microsoft 登录，请检查 Supported accounts 配置。".into(),
        _ => oauth_error(error),
    };
    match code {
        Some(code) => format!("{message}（AADSTS{code}）"),
        None => message,
    }
}
fn xbox_error(code: u64) -> String {
    match code {
        2148916233 => "此账号尚未建立 Xbox 档案，请先在 Xbox 官方网站完成设置。".into(),
        2148916238 | 2148916229 => {
            "此账号受家庭权限限制，请由监护人在 Microsoft 家庭设置中完成授权。".into()
        }
        2148916236 | 2148916237 => {
            "Xbox 要求验证年龄或限制了游戏时间，请自行前往官方账号页面处理。".into()
        }
        2148916235 => "Xbox 服务在该账号所在地区不可用。".into(),
        _ => format!("Xbox 身份认证失败（XErr {code}）。"),
    }
}
pub fn validate_client_id(id: &str) -> Result<()> {
    uuid::Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| "Microsoft Client ID 应为有效的应用 UUID。".into())
}
pub fn valid_profile_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err("登录已取消。".into())
    } else {
        Ok(())
    }
}
fn wait(seconds: u64, deadline: Instant, cancel: &AtomicBool) -> Result<()> {
    let until = Instant::now() + Duration::from_secs(seconds);
    loop {
        check_cancel(cancel)?;
        if Instant::now() >= deadline {
            return Err("登录代码已过期，请重新登录。".into());
        }
        if Instant::now() >= until {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests;
