use crate::app::NsclApp;
use nscl::external_auth::{Client, Login, Profile, Server, Session};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, TryRecvError},
};
enum Reply {
    Server(Server),
    Profiles(Box<Login>),
    Session(Box<Session>),
}
enum Action {
    Discover(String),
    Login(Server, String, String),
    Select(Login, Profile),
}
#[derive(Default)]
pub struct View {
    root: String,
    username: String,
    password: String,
    server: Option<Server>,
    login: Option<Login>,
    profile: usize,
    pending: Option<Receiver<Result<Reply, String>>>,
    cancel: Option<Arc<AtomicBool>>,
    notice: String,
}
fn start(app: &mut NsclApp, action: Action, ctx: &egui::Context) {
    let Some(cancel) = app.accounts.begin_external() else {
        return;
    };
    let (tx, rx) = mpsc::channel();
    app.external_login.pending = Some(rx);
    app.external_login.cancel = Some(cancel.clone());
    app.external_login.notice.clear();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let result = Client::new().and_then(|client| match action {
            Action::Discover(root) => client.discover(&root, &cancel).map(Reply::Server),
            Action::Login(server, name, password) => {
                let login = client.login(&server, &name, &password, &cancel)?;
                if login.selected() {
                    client
                        .select(&login, None, &cancel)
                        .map(|s| Reply::Session(Box::new(s)))
                } else {
                    Ok(Reply::Profiles(Box::new(login)))
                }
            }
            Action::Select(login, profile) => client
                .select(&login, Some(&profile), &cancel)
                .map(|s| Reply::Session(Box::new(s))),
        });
        let _ = tx.send(result);
        ctx.request_repaint();
    });
}
pub fn poll(app: &mut NsclApp) {
    let Some(rx) = &app.external_login.pending else {
        return;
    };
    let result = match rx.try_recv() {
        Ok(result) => result,
        Err(TryRecvError::Empty) => return,
        Err(TryRecvError::Disconnected) => Err("外置登录任务提前结束，请重试。".into()),
    };
    app.external_login.pending = None;
    app.accounts.pending = false;
    if app
        .external_login
        .cancel
        .take()
        .is_some_and(|c| c.load(Ordering::Relaxed))
    {
        app.external_login.notice = "操作已取消。".into();
        app.external_login.login = None;
        return;
    }
    match result {
        Ok(Reply::Server(server)) => {
            app.external_login.server = Some(server);
            app.external_login.login = None;
            app.external_login.notice =
                "服务器信息已读取，请核对认证 API 地址，再填写此服务器的账号和密码。".into();
        }
        Ok(Reply::Profiles(login)) => {
            app.external_login.login = Some(*login);
            app.external_login.profile = 0;
            app.external_login.notice = "请选择此账号的角色。".into();
        }
        Ok(Reply::Session(session)) => {
            app.accounts.accept_external(*session);
            app.settings.online_mode = true;
            if let Err(e) = app.accounts.save_mode(&app.settings) {
                app.accounts.error = Some(e);
            }
            app.external_login.notice = app.accounts.status.clone();
            app.external_login.login = None;
        }
        Err(e) => app.external_login.notice = e,
    }
}
pub fn show(ui: &mut egui::Ui, app: &mut NsclApp) {
    let mut action = None;
    ui.strong("外置认证（Yggdrasil / authlib-injector）");
    ui.label("填写认证站提供的 HTTPS API 地址，先读取并核对服务器信息。密码仅用于这次登录；记住登录状态时仅保存令牌。");
    let editable = !app.accounts.pending && !app.service.busy() && app.service.game.is_none();
    ui.add_enabled_ui(editable, |ui| {
        if ui
            .add(
                egui::TextEdit::singleline(&mut app.external_login.root)
                    .hint_text("https://认证站/api/yggdrasil/")
                    .desired_width(ui.available_width()),
            )
            .changed()
        {
            app.external_login.server = None;
            app.external_login.login = None;
            app.external_login.password.clear();
        }
        if ui.button("读取认证服务器信息").clicked() {
            action = Some(Action::Discover(app.external_login.root.clone()));
        }
        if let Some(server) = &app.external_login.server {
            ui.strong(&server.name);
            ui.label(format!("认证 API：{}", server.root));
            ui.add(
                egui::TextEdit::singleline(&mut app.external_login.username)
                    .hint_text("此认证站的账号"),
            );
            ui.add(
                egui::TextEdit::singleline(&mut app.external_login.password)
                    .password(true)
                    .hint_text("此认证站的密码"),
            );
            if ui.button("确认此服务器并登录").clicked() {
                action = Some(Action::Login(
                    server.clone(),
                    app.external_login.username.clone(),
                    std::mem::take(&mut app.external_login.password),
                ));
            }
        }
        if let Some(login) = &app.external_login.login {
            egui::ComboBox::from_id_salt("external-profile")
                .selected_text(
                    login
                        .profiles
                        .get(app.external_login.profile)
                        .map(|p| p.name.as_str())
                        .unwrap_or("选择角色"),
                )
                .show_ui(ui, |ui| {
                    for (index, p) in login.profiles.iter().enumerate() {
                        ui.selectable_value(&mut app.external_login.profile, index, &p.name);
                    }
                });
            if ui.button("使用所选角色").clicked()
                && let Some(profile) = login.profiles.get(app.external_login.profile)
            {
                action = Some(Action::Select(login.clone(), profile.clone()));
            }
        }
    });
    if app.external_login.pending.is_some() {
        ui.spinner();
        if ui.button("取消外置登录").clicked() {
            app.accounts.cancel();
        }
    }
    if !app.external_login.notice.is_empty() {
        ui.label(&app.external_login.notice);
    }
    ui.label("外置账号用于支持该认证站的服务器。Microsoft 账号与离线账号仍可在账号管理页切换。");
    if let Some(action) = action {
        start(app, action, ui.ctx());
    }
}
