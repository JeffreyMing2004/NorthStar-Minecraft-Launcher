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

pub struct LoginPrompt {
    pub code: String,
    pub url: String,
    pub expires: Instant,
}
enum Event {
    Prompt(LoginPrompt),
    Browser(accounts::BrowserPrompt),
    Status(String),
    Finished(Result<(Session, Option<String>), String>),
}

pub struct AccountService {
    pub list: AccountList,
    pub pending: bool,
    pub prompt: Option<LoginPrompt>,
    pub browser_prompt: Option<accounts::BrowserPrompt>,
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
    pub fn poll(&mut self, settings: &mut Settings) {
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
                    self.status = match open::that_detached(&prompt.url) {
                        Ok(()) => {
                            "请在浏览器中完成 Microsoft 登录，授权后会自动返回启动器。".into()
                        }
                        Err(_) => "无法自动打开浏览器，请点击下方的 Microsoft 登录链接。".into(),
                    };
                    self.browser_prompt = Some(prompt);
                }
                Event::Status(status) => {
                    self.prompt = None;
                    self.browser_prompt = None;
                    self.status = status;
                }
                Event::Finished(result) => {
                    self.pending = false;
                    self.prompt = None;
                    self.browser_prompt = None;
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
