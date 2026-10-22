use nscl::{
    accounts::{self, Account, AccountList, AuthClient, CredentialStore, Session, SystemVault},
    store::{self, Settings},
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    time::Instant,
};

#[derive(Clone)]
pub struct LoginPrompt {
    pub code: String,
    pub url: String,
    pub expires: Instant,
}
enum Event {
    Prompt(LoginPrompt),
    Browser(accounts::BrowserPrompt),
    BrowserOpened {
        login: Arc<AtomicBool>,
        result: Result<(), String>,
    },
    Status(String),
    Finished(Result<(Session, Option<String>), String>),
}

pub struct AccountService {
    pub list: AccountList,
    pub pending: bool,
    pub prompt: Option<LoginPrompt>,
    pub browser_prompt: Option<accounts::BrowserPrompt>,
    pub browser_opening: bool,
    pub status: String,
    pub error: Option<String>,
    sessions: BTreeMap<String, Session>,
    cancel: Arc<AtomicBool>,
    sender: Sender<Event>,
    receiver: Receiver<Event>,
}
impl AccountService {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        let (list, error) = match AccountList::load() {
            Ok(list) => (list, None),
            Err(error) => (AccountList::default(), Some(error)),
        };
        Self {
            list,
            pending: false,
            prompt: None,
            browser_prompt: None,
            browser_opening: false,
            status: String::new(),
            error,
            sessions: BTreeMap::new(),
            cancel: Arc::new(AtomicBool::new(false)),
            sender,
            receiver,
        }
    }
    pub fn launch_account(&self) -> Option<(Account, Option<Session>)> {
        self.list
            .current()
            .map(|account| (account.clone(), self.sessions.get(&account.id).cloned()))
    }
    pub fn accept(&mut self, session: Session) {
        let mut list = self.list.clone();
        list.upsert(session.account.clone());
        if let Err(error) = list.save() {
            self.error = Some(error);
        }
        self.list = list;
        self.sessions.insert(session.account.id.clone(), session);
    }
    pub fn login(&mut self, client_id: &str, redirect_uri: &str, ctx: &egui::Context) {
        if let Err(error) = accounts::validate_redirect_uri(redirect_uri.trim()) {
            self.error = Some(error);
            return;
        }
        self.start_login(client_id, Some(redirect_uri.trim().into()), ctx);
    }
    pub fn login_device(&mut self, client_id: &str, ctx: &egui::Context) {
        self.start_login(client_id, None, ctx);
    }
    fn start_login(&mut self, client_id: &str, redirect: Option<String>, ctx: &egui::Context) {
        if self.pending {
            return;
        }
        if let Err(error) = accounts::validate_client_id(client_id.trim()) {
            self.error = Some(error);
            return;
        }
        self.pending = true;
        self.prompt = None;
        self.browser_prompt = None;
        self.browser_opening = false;
        self.error = None;
        self.status = if redirect.is_some() {
            "正在准备 Microsoft 浏览器登录……"
        } else {
            "正在申请 Microsoft 设备代码……"
        }
        .into();
        self.cancel = Arc::new(AtomicBool::new(false));
        let (sender, ctx, cancel, client_id) = (
            self.sender.clone(),
            ctx.clone(),
            self.cancel.clone(),
            client_id.trim().to_owned(),
        );
        std::thread::spawn(move || {
            let result = AuthClient::new()
                .and_then(|client| {
                    let stage = |stage: &str| {
                        let _ = sender.send(Event::Status(stage.into()));
                        ctx.request_repaint();
                    };
                    if let Some(redirect) = redirect {
                        client.login_browser(
                            &client_id,
                            &redirect,
                            &cancel,
                            |prompt| {
                                let _ = sender.send(Event::Browser(prompt.clone()));
                                ctx.request_repaint();
                            },
                            stage,
                        )
                    } else {
                        client.login(
                            &client_id,
                            &cancel,
                            |code| {
                                let _ = sender.send(Event::Prompt(LoginPrompt {
                                    code: code.user_code.clone(),
                                    url: code.verification_uri.clone(),
                                    expires: Instant::now()
                                        + std::time::Duration::from_secs(code.expires_in),
                                }));
                                ctx.request_repaint();
                            },
                            stage,
                        )
                    }
                })
                .and_then(|session| {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("登录已取消。".into());
                    }
                    let warning = SystemVault
                        .write(&session.account.id, &session.credential)
                        .err();
                    Ok((session, warning))
                });
            let _ = sender.send(Event::Finished(result));
            ctx.request_repaint();
        });
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    pub fn open_browser(&mut self, ctx: &egui::Context) {
        if !self.pending || self.browser_opening || self.cancel.load(Ordering::Relaxed) {
            return;
        }
        let Some(url) = self.login_url() else {
            return;
        };
        let url = url.to_owned();
        self.browser_opening = true;
        self.status = "正在打开系统默认浏览器……".into();
        let (sender, login, ctx) = (self.sender.clone(), self.cancel.clone(), ctx.clone());
        std::thread::spawn(move || {
            if login.load(Ordering::Relaxed) {
                return;
            }
            let result = nscl::browser::open(&url);
            let _ = sender.send(Event::BrowserOpened { login, result });
            ctx.request_repaint();
        });
    }
    pub fn copy_login_link(&mut self, ctx: &egui::Context) {
        if let Some(url) = self.login_url() {
            ctx.copy_text(url.to_owned());
            self.status = "登录链接已复制，请粘贴到浏览器地址栏，完成授权后返回启动器。".into();
        }
    }
    fn login_url(&self) -> Option<&str> {
        self.browser_prompt
            .as_ref()
            .map(|prompt| prompt.url.as_str())
            .or_else(|| self.prompt.as_ref().map(|prompt| prompt.url.as_str()))
    }
    pub fn poll(&mut self, settings: &mut Settings, ctx: &egui::Context) {
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                Event::Prompt(prompt) => {
                    if self.cancel.load(Ordering::Relaxed) {
                        continue;
                    }
                    self.prompt = Some(prompt);
                    self.status = "请在 Microsoft 官方页面输入设备代码并完成授权。".into();
                }
                Event::Browser(prompt) => {
                    if self.cancel.load(Ordering::Relaxed) {
                        continue;
                    }
                    self.browser_prompt = Some(prompt);
                    self.open_browser(ctx);
                }
                Event::BrowserOpened { login, result } => {
                    // A handler can finish after cancellation, code exchange, or a new
                    // login. It must not replace that operation's progress or result.
                    if !Arc::ptr_eq(&login, &self.cancel)
                        || login.load(Ordering::Relaxed)
                        || !self.pending
                        || self.login_url().is_none()
                    {
                        continue;
                    }
                    self.browser_opening = false;
                    self.status = match result {
                        Ok(()) => "已请求打开默认浏览器，请完成 Microsoft 授权。若未显示，可重新打开或复制登录链接。".into(),
                        Err(error) => format!("无法自动打开浏览器：{error}"),
                    };
                }
                Event::Status(status) => {
                    self.prompt = None;
                    self.browser_prompt = None;
                    self.browser_opening = false;
                    self.status = status;
                }
                Event::Finished(result) => {
                    self.pending = false;
                    self.prompt = None;
                    self.browser_prompt = None;
                    self.browser_opening = false;
                    match result {
                        Ok((session, warning)) => {
                            self.status = format!(
                                "{} 已通过 Minecraft Java 正版验证。{}",
                                session.account.name,
                                warning.unwrap_or_default()
                            );
                            self.accept(session);
                            settings.online_mode = true;
                            if let Err(error) = self.save_mode(settings) {
                                self.error = Some(error);
                            }
                        }
                        Err(error) if error == "登录已取消。" => self.status = error,
                        Err(error) => {
                            self.status.clear();
                            self.error = Some(error);
                        }
                    }
                }
            }
        }
    }
    pub fn select(&mut self, id: &str, settings: &mut Settings) -> Result<(), String> {
        if self.pending {
            return Err("请先完成或取消登录。".into());
        }
        if !self.list.accounts.iter().any(|a| a.id == id) {
            return Err("账号不存在。".into());
        }
        let mut list = self.list.clone();
        list.selected = Some(id.into());
        list.save()?;
        self.list = list;
        settings.online_mode = true;
        self.save_mode(settings)
    }
    pub fn remove(&mut self, id: &str, settings: &mut Settings) -> Result<(), String> {
        if self.pending {
            return Err("请先完成或取消登录。".into());
        }
        SystemVault.remove(id)?;
        let mut list = self.list.clone();
        list.accounts.retain(|a| a.id != id);
        if list.selected.as_deref() == Some(id) {
            list.selected = list.accounts.first().map(|a| a.id.clone());
        }
        list.save()?;
        self.list = list;
        self.sessions.remove(id);
        self.save_mode(settings)?;
        self.status = "已移除该账号及本机保存的刷新令牌。".into();
        Ok(())
    }
    pub fn save_mode(&self, settings: &Settings) -> Result<(), String> {
        let (mut saved, _) = store::load();
        saved.online_mode = settings.online_mode;
        store::save(&saved)
    }
}

impl Drop for AccountService {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn waiting_login() -> AccountService {
        let (sender, receiver) = mpsc::channel();
        AccountService {
            list: AccountList::default(),
            pending: true,
            prompt: None,
            browser_prompt: Some(accounts::BrowserPrompt {
                url: "https://login.live.com/?state=test%26state&code_challenge=abc-def".into(),
                redirect_uri: accounts::DEFAULT_REDIRECT_URI.into(),
                expires: Instant::now() + std::time::Duration::from_secs(600),
            }),
            browser_opening: true,
            status: "正在打开系统默认浏览器……".into(),
            error: None,
            sessions: BTreeMap::new(),
            cancel: Arc::new(AtomicBool::new(false)),
            sender,
            receiver,
        }
    }

    #[test]
    fn browser_failure_keeps_login_and_exact_link_for_clipboard_recovery() {
        let mut service = waiting_login();
        let url = service.login_url().unwrap().to_owned();
        service
            .sender
            .send(Event::BrowserOpened {
                login: service.cancel.clone(),
                result: Err("系统错误 1155；请复制登录链接到浏览器中打开。".into()),
            })
            .unwrap();
        let ctx = egui::Context::default();
        service.poll(&mut Settings::default(), &ctx);
        assert!(service.pending);
        assert!(!service.browser_opening);
        assert!(service.status.contains("1155"));
        assert_eq!(service.login_url(), Some(url.as_str()));
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            service.copy_login_link(ui.ctx())
        });
        assert!(
            output.platform_output.commands.iter().any(
                |command| matches!(command, egui::OutputCommand::CopyText(text) if text.as_str() == url.as_str())
            )
        );
        output.drop_without_applying_deltas();
    }

    #[test]
    fn late_browser_result_cannot_replace_token_exchange_progress() {
        let mut service = waiting_login();
        service
            .sender
            .send(Event::Status("正在验证 Xbox 身份……".into()))
            .unwrap();
        service
            .sender
            .send(Event::BrowserOpened {
                login: service.cancel.clone(),
                result: Err("迟到的浏览器错误".into()),
            })
            .unwrap();
        service.poll(&mut Settings::default(), &egui::Context::default());
        assert_eq!(service.status, "正在验证 Xbox 身份……");
        assert!(service.login_url().is_none());
        assert!(!service.browser_opening);
    }

    #[test]
    fn browser_results_from_cancelled_or_previous_login_are_ignored() {
        for cancelled in [true, false] {
            let mut service = waiting_login();
            let login = service.cancel.clone();
            if cancelled {
                service.cancel();
            } else {
                service.cancel = Arc::new(AtomicBool::new(false));
            }
            service
                .sender
                .send(Event::BrowserOpened {
                    login,
                    result: Err("不属于当前登录的错误".into()),
                })
                .unwrap();
            service.poll(&mut Settings::default(), &egui::Context::default());
            assert_eq!(service.status, "正在打开系统默认浏览器……");
            assert!(service.browser_opening);
        }
    }
}
