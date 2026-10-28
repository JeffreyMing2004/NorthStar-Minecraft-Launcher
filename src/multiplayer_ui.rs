use crate::{
    app::{self, NsclApp},
    pages::download_button,
    theme,
};
use nscl::multiplayer::frpc::{FrpcSession, HostOptions};
use nscl::{
    accounts::Account,
    multiplayer::{RelayClient, RelaySession},
};
use std::{
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
};

enum Session {
    Frpc(FrpcSession),
    Legacy(RelaySession),
}
impl Session {
    fn room(&self) -> &str {
        match self {
            Self::Frpc(s) => &s.room,
            Self::Legacy(s) => &s.room,
        }
    }
    fn hosting(&self) -> bool {
        match self {
            Self::Frpc(s) => s.hosting,
            Self::Legacy(s) => s.hosting,
        }
    }
    fn address(&self) -> Option<SocketAddr> {
        match self {
            Self::Frpc(s) => s.local_address,
            Self::Legacy(s) => s.local_address,
        }
    }
    fn connections(&self) -> usize {
        match self {
            Self::Frpc(s) => s.connections(),
            Self::Legacy(s) => s.connections(),
        }
    }
    fn message(&self) -> String {
        match self {
            Self::Frpc(s) => s.message(),
            Self::Legacy(s) => s.message(),
        }
    }
    fn running(&self) -> bool {
        match self {
            Self::Frpc(s) => s.running(),
            Self::Legacy(s) => s.running(),
        }
    }
}

#[derive(Default)]
pub struct MultiplayerPage {
    token: String,
    game_port: String,
    room_code: String,
    message: String,
    identity: Option<String>,
    pending: Option<Receiver<Result<Session, String>>>,
    pending_cancel: Option<Arc<AtomicBool>>,
    progress: Arc<Mutex<String>>,
    session: Option<Session>,
}

impl MultiplayerPage {
    pub fn poll(&mut self, account: Option<&Account>) {
        let current = account.map(|a| a.id.clone());
        if self.identity != current {
            if let Some(cancel) = self.pending_cancel.take() {
                cancel.store(true, Ordering::Relaxed);
            }
            self.session = None;
            self.pending = None;
            self.token.clear();
            self.message.clear();
            self.identity = current;
        }
        if let Some(receiver) = &self.pending {
            match receiver.try_recv() {
                Ok(result) => {
                    self.pending = None;
                    let cancelled = self
                        .pending_cancel
                        .take()
                        .is_some_and(|c| c.load(Ordering::Relaxed));
                    if cancelled {
                        drop(result);
                        self.message = "操作已取消。".into();
                        return;
                    }
                    match result {
                        Ok(session) => {
                            self.message = "联机连接已就绪。".into();
                            self.session = Some(session);
                        }
                        Err(error) => self.message = error,
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.pending_cancel = None;
                    self.message = "联机任务已结束，请重试。".into();
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
    }

    fn start(
        &mut self,
        endpoint: String,
        account: Account,
        hosting: bool,
        executable: String,
        ctx: &egui::Context,
    ) {
        if self.pending.is_some() || self.session.is_some() {
            return;
        }
        let client = match RelayClient::new(&endpoint, self.token.trim()) {
            Ok(client) => client,
            Err(error) => {
                self.message = error;
                return;
            }
        };
        let port = if hosting {
            match self.game_port.trim().parse::<u16>() {
                Ok(port) if port > 0 => port,
                _ => {
                    self.message = "请填写游戏内显示的局域网端口（1–65535）。".into();
                    return;
                }
            }
        } else {
            0
        };
        let (sender, receiver) = mpsc::channel();
        self.pending = Some(receiver);
        self.message = "正在验证联机权限并连接房间……".into();
        let cancel = Arc::new(AtomicBool::new(false));
        self.pending_cancel = Some(cancel.clone());
        self.progress = Arc::new(Mutex::new(self.message.clone()));
        let progress = self.progress.clone();
        let (room, ctx) = (self.room_code.clone(), ctx.clone());
        std::thread::spawn(move || {
            let result = if hosting {
                let report_ctx = ctx.clone();
                client
                    .host_frpc(
                        &account,
                        port,
                        HostOptions {
                            executable: std::path::PathBuf::from(
                                executable.trim().trim_matches('"'),
                            ),
                            runtime_directory: crate::store::settings_path()
                                .with_file_name("relay-data")
                                .join("frpc-runtime"),
                            cancel,
                            report: Arc::new(move |message| {
                                *progress.lock().unwrap() = message;
                                report_ctx.request_repaint();
                            }),
                        },
                    )
                    .map(Session::Frpc)
            } else if room.trim().starts_with("frpc:") {
                client.join_frpc(&account, &room, cancel).map(Session::Frpc)
            } else {
                client.join(&account, &room).map(Session::Legacy)
            };
            let _ = sender.send(result);
            ctx.request_repaint();
        });
    }
}

impl Drop for MultiplayerPage {
    fn drop(&mut self) {
        if let Some(cancel) = &self.pending_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}

pub fn show(ui: &mut egui::Ui, app: &mut NsclApp) {
    let account = app.accounts.list.current().cloned();
    app.multiplayer.poll(account.as_ref());
    app::heading(ui, "联机");
    ui.add_space(8.0);
    app::body(
        ui,
        "房主通过 frpc 连接 88frp，自动测速选择节点，与白名单中的朋友一起游玩。",
    );
    app::hint(
        ui,
        "服务仅向管理员登记的最多 30 个正版或离线账号开放，创建和加入都需要独立联机凭证。",
    );
    ui.add_space(18.0);
    let busy = app.multiplayer.pending.is_some() || app.multiplayer.session.is_some();
    theme::card(18).show(ui, |ui| {
        ui.set_width((ui.available_width() - 4.0).max(0.0));
        ui.spacing_mut().item_spacing.y = 10.0;
        app::body(ui, &account.as_ref().map(|a| format!("当前账号：{}", a.name)).unwrap_or_else(|| "请先在账号页选择正版或离线账号。".into()));
        ui.add_enabled_ui(!busy, |ui| {
            ui.label("联机服务地址");
            ui.add(egui::TextEdit::singleline(&mut app.settings.multiplayer_endpoint)
                .hint_text("管理员提供的 https://域名:端口")
                .desired_width(ui.available_width()));
            ui.label("此账号的联机凭证");
            ui.add(egui::TextEdit::singleline(&mut app.multiplayer.token).password(true)
                .hint_text("由联机服务管理员提供")
                .desired_width(ui.available_width()));
            app::hint(ui, "联机凭证仅保留在本次运行内，不会保存到设置或导出包。");
            egui::CollapsingHeader::new("房主 frpc 程序设置").show(ui,|ui| {
                ui.label("frpc 程序路径（仅房主需要）");
                ui.add(egui::TextEdit::singleline(&mut app.settings.frpc_path).hint_text("选择或输入本地 frpc.exe 完整路径").desired_width(ui.available_width()));
                if download_button(ui,"选择 frpc 程序",cfg!(windows)) {
                    match crate::file_picker::frpc() {
                        Ok(Some(path))=>app.settings.frpc_path=path.to_string_lossy().into_owned(),
                        Ok(None)=>{},Err(error)=>app.multiplayer.message=error,
                    }
                }
            });
            theme::toolbar(ui, |ui| {
                if download_button(ui, "保存服务地址", true) {
                    app.multiplayer.message = match crate::store::save(&app.settings) {
                        Ok(()) => "联机服务地址已保存。".into(), Err(error) => error,
                    };
                }
                if download_button(ui, "88frp 帮助", true)
                    && webbrowser::open("https://88frp.com/client/help-support/FRPC%20%E9%85%8D%E7%BD%AE%E6%8C%87%E5%8D%97").is_err()
                {app.multiplayer.message = "无法打开 88frp 帮助页面。".into();}
            });
        });
    });
    ui.add_space(16.0);
    if let Some(session) = &app.multiplayer.session {
        theme::card(18).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            app::body(ui, if session.hosting() {"我的房间"} else {"已加入房间"});
            app::body(ui, &format!("房间码：{}", session.room()));
            if let Session::Frpc(frpc)=session {
                let latency=frpc.latency_ms.map(|n|format!(" · TCP 延迟 {n} ms")).unwrap_or_default();
                app::body(ui,&format!("节点：{}{latency}",frpc.node_name));
                app::hint(ui,&format!("隧道地址：{}（好友请通过房间码授权加入）",frpc.public_address));
            }
            if let Some(address) = session.address() {
                app::body(ui, &format!("游戏连接地址：{address}"));
                app::hint(ui, "在 Minecraft「多人游戏 → 直接连接」中输入此地址，并保持启动器开启。");
            } else {
                app::hint(ui, "将房间码分享给白名单好友，对方在启动器中加入。房主需保持游戏、局域网世界和启动器开启。");
            }
            app::hint(ui, &format!("当前数据连接：{}", session.connections()));
            let message = session.message();
            if !message.is_empty() {app::hint(ui, &message);}
            if !session.running() {app::hint(ui, "联机已停止，请关闭后重新连接。");}
            theme::toolbar(ui, |ui| {
                if download_button(ui, "复制房间码", true) {ui.ctx().copy_text(session.room().to_owned());}
                if let Some(address) = session.address()
                    && download_button(ui, "复制连接地址", true)
                {ui.ctx().copy_text(address.to_string());}
            });
        });
        ui.add_space(12.0);
        if download_button(ui, "断开联机", true) {
            app.multiplayer.session = None;
            app.multiplayer.message = "已断开联机。".into();
        }
    } else {
        theme::card(18).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.add_enabled_ui(!busy && account.is_some(), |ui| {
                app::body(ui, "创建房间");
                app::hint(ui, "先启动游戏并进入世界，选择「对局域网开放」，填入游戏显示的端口。好友需使用兼容的游戏版本和模组。");
                ui.horizontal(|ui| {
                    ui.label("游戏局域网端口");
                    ui.add(egui::TextEdit::singleline(&mut app.multiplayer.game_port).hint_text("例如 51234").desired_width(130.0));
                });
                if app::primary_button(ui, "创建联机房间") {
                    app.multiplayer.start(app.settings.multiplayer_endpoint.clone(), account.clone().unwrap(), true,app.settings.frpc_path.clone(), ui.ctx());
                }
                ui.add_space(14.0);
                app::body(ui, "加入房间");
                ui.add(egui::TextEdit::singleline(&mut app.multiplayer.room_code).hint_text("粘贴朋友分享的房间码").desired_width(ui.available_width()));
                if download_button(ui, "加入联机房间", true) {
                    app.multiplayer.start(app.settings.multiplayer_endpoint.clone(), account.clone().unwrap(), false,String::new(), ui.ctx());
                }
            });
        });
    }
    if !app.multiplayer.message.is_empty() {
        ui.add_space(12.0);
        let message = if app.multiplayer.pending.is_some() {
            app.multiplayer.progress.lock().unwrap().clone()
        } else {
            app.multiplayer.message.clone()
        };
        app::hint(ui, &message);
    }
    if app.multiplayer.pending.is_some()
        && download_button(ui, "取消连接", true)
        && let Some(cancel) = &app.multiplayer.pending_cancel
    {
        cancel.store(true, Ordering::Relaxed);
    }
    if busy {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(250));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Page;

    #[test]
    fn multiplayer_page_is_available_and_account_changes_clear_credentials() {
        assert!(Page::ALL.contains(&Page::Multiplayer));
        let alice = Account::offline("Alice").unwrap();
        let bob = Account::offline("Bob").unwrap();
        let mut page = MultiplayerPage::default();
        page.poll(Some(&alice));
        page.token = "a".repeat(64);
        page.message = "old account".into();
        page.poll(Some(&alice));
        assert_eq!(page.token.len(), 64);
        page.poll(Some(&bob));
        assert!(page.token.is_empty());
        assert!(page.message.is_empty());
        page.token = "b".repeat(64);
        page.poll(None);
        assert!(page.token.is_empty());
    }

    #[test]
    fn multiplayer_page_renders_in_the_minimum_window_and_does_not_persist_credentials() {
        let (_root, mut app, ctx) = crate::service::tests::fixture();
        app.page = Page::Multiplayer;
        let alice = Account::offline("Alice").unwrap();
        app.accounts.list.accounts = vec![alice.clone()];
        app.accounts.list.selected = Some(alice.id);
        app.multiplayer.poll(app.accounts.list.current());
        app.multiplayer.token = "a".repeat(64);
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(960.0, 640.0),
                )),
                ..Default::default()
            },
            |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| show(ui, &mut app));
            },
        );
        for label in [
            "联机",
            "当前账号：Alice",
            "联机服务地址",
            "此账号的联机凭证",
            "创建联机房间",
            "加入联机房间",
        ] {
            crate::service::tests::text_rect(&output, label);
        }
        assert!(
            !serde_json::to_string(&app.settings)
                .unwrap()
                .contains(&"a".repeat(64))
        );
        output.drop_without_applying_deltas();
    }
}
