use crate::{
    app::{self, NsclApp},
    pages::download_button,
    theme,
};
use nscl::multiplayer::frpc::download as frpc_download;
use nscl::multiplayer::frpc::{FrpcSession, HostOptions};
use nscl::{accounts::Account, multiplayer::RelayClient};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
};

struct Session(FrpcSession);
enum Completion {
    Connected {
        result: Result<Session, String>,
        program: Option<PathBuf>,
    },
    Downloaded(Result<PathBuf, String>),
    Checked(Result<String, String>),
}
impl Session {
    fn room(&self) -> &str {
        &self.0.room
    }
    fn hosting(&self) -> bool {
        self.0.hosting
    }
    fn address(&self) -> Option<SocketAddr> {
        self.0.local_address
    }
    fn connections(&self) -> usize {
        self.0.connections()
    }
    fn message(&self) -> String {
        self.0.message()
    }
    fn running(&self) -> bool {
        self.0.running()
    }
}

#[derive(Default)]
pub struct MultiplayerPage {
    token: String,
    game_port: String,
    room_code: String,
    message: String,
    identity: Option<String>,
    pending: Option<Receiver<Completion>>,
    pending_cancel: Option<Arc<AtomicBool>>,
    progress: Arc<Mutex<String>>,
    session: Option<Session>,
    entered: bool,
    download_prompt: bool,
    ready_program: Option<PathBuf>,
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
                Ok(completion) => {
                    self.pending = None;
                    let cancelled = self
                        .pending_cancel
                        .take()
                        .is_some_and(|c| c.load(Ordering::Relaxed));
                    if cancelled {
                        drop(completion);
                        self.message = "操作已取消。".into();
                        return;
                    }
                    let result = match completion {
                        Completion::Checked(result) => {
                            self.message = result.unwrap_or_else(|error| error);
                            return;
                        }
                        Completion::Downloaded(result) => {
                            match result {
                                Ok(path) => {
                                    self.ready_program = Some(path);
                                    self.download_prompt = false;
                                    self.message = "frpc 已下载并校验完成，可以创建房间。".into();
                                }
                                Err(error) => self.message = error,
                            }
                            return;
                        }
                        Completion::Connected { result, program } => {
                            if program.is_some() {
                                self.ready_program = program;
                            }
                            result
                        }
                    };
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
    pub fn leave(&mut self) {
        self.entered = false;
        self.download_prompt = false;
    }
    fn check_program(&mut self, path: &str, directory: &Path) {
        if self.entered {
            return;
        }
        self.entered = true;
        if !Path::new(path.trim().trim_matches('"')).is_file() {
            if let Some(path) = frpc_download::installed(directory) {
                self.ready_program = Some(path);
            } else {
                self.download_prompt = true;
            }
        }
    }
    fn download(&mut self, ctx: &egui::Context) {
        if self.pending.is_some() || self.session.is_some() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.pending = Some(receiver);
        self.download_prompt = false;
        let cancel = Arc::new(AtomicBool::new(false));
        self.pending_cancel = Some(cancel.clone());
        self.message = "正在下载 frpc……".into();
        self.progress = Arc::new(Mutex::new(self.message.clone()));
        let (progress, ctx) = (self.progress.clone(), ctx.clone());
        std::thread::spawn(move || {
            let report_ctx = ctx.clone();
            let report = move |message| {
                *progress.lock().unwrap() = message;
                report_ctx.request_repaint();
            };
            let result = frpc_download::install(&frpc_download::directory(), &cancel, &report);
            let _ = sender.send(Completion::Downloaded(result));
            ctx.request_repaint();
        });
    }

    fn check_service(&mut self, endpoint: String, ctx: &egui::Context) {
        if self.pending.is_some() || self.session.is_some() {
            return;
        }
        let client = match RelayClient::public(&endpoint) {
            Ok(client) => client,
            Err(error) => {
                self.message = error;
                return;
            }
        };
        let (sender, receiver) = mpsc::channel();
        self.pending = Some(receiver);
        self.pending_cancel = Some(Arc::new(AtomicBool::new(false)));
        self.message = "正在检查验证服务……".into();
        self.progress = Arc::new(Mutex::new(self.message.clone()));
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = client.check_service().map(|info| {
                format!(
                    "验证服务已就绪：{} 个节点、{} 条隧道，最多 {} 个房主同时开房。",
                    info.nodes, info.tunnels, info.max_hosts
                )
            });
            let _ = sender.send(Completion::Checked(result));
            ctx.request_repaint();
        });
    }

    fn start(
        &mut self,
        endpoint: String,
        account: Option<Account>,
        hosting: bool,
        executable: String,
        ctx: &egui::Context,
    ) {
        if self.pending.is_some() || self.session.is_some() {
            return;
        }
        let client = match if hosting {
            RelayClient::new(&endpoint, self.token.trim())
        } else {
            RelayClient::public(&endpoint)
        } {
            Ok(client) => client,
            Err(error) => {
                self.message = error;
                return;
            }
        };
        if hosting && account.is_none() {
            self.message = "请先选择房主的启动器账号。".into();
            return;
        }
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
        self.message = if hosting {
            "正在准备房主联机……"
        } else {
            "正在加入房间……"
        }
        .into();
        let cancel = Arc::new(AtomicBool::new(false));
        self.pending_cancel = Some(cancel.clone());
        self.progress = Arc::new(Mutex::new(self.message.clone()));
        let progress = self.progress.clone();
        let (room, ctx) = (self.room_code.clone(), ctx.clone());
        std::thread::spawn(move || {
            let mut program = None;
            let result = if hosting {
                let report_ctx = ctx.clone();
                let report: Arc<dyn Fn(String) + Send + Sync> = Arc::new(move |message| {
                    *progress.lock().unwrap() = message;
                    report_ctx.request_repaint();
                });
                let selected = PathBuf::from(executable.trim().trim_matches('"'));
                let executable = if selected.is_file() {
                    Ok(selected)
                } else {
                    frpc_download::install(&frpc_download::directory(), &cancel, report.as_ref())
                };
                executable.and_then(|executable| {
                    program = Some(executable.clone());
                    client
                        .host_frpc(
                            account.as_ref().unwrap(),
                            port,
                            HostOptions {
                                executable,
                                runtime_directory: crate::store::settings_path()
                                    .with_file_name("relay-data")
                                    .join("frpc-runtime"),
                                cancel,
                                report,
                            },
                        )
                        .map(Session)
                })
            } else if room.trim().starts_with("frpc:") {
                client.join_frpc_room(&room, cancel).map(Session)
            } else {
                Err("请填写好友分享的 frpc 房间码。".into())
            };
            let _ = sender.send(Completion::Connected { result, program });
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
    app.multiplayer
        .check_program(&app.settings.frpc_path, &frpc_download::directory());
    if let Some(path) = app.multiplayer.ready_program.take() {
        app.settings.frpc_path = path.to_string_lossy().into_owned();
        if let Err(error) = crate::store::save(&app.settings) {
            app.multiplayer.message = error;
        }
    }
    app::heading(ui, "联机");
    ui.add_space(8.0);
    app::body(ui, "房主通过 frpc 连接 88frp，自动测速并分配空闲隧道。");
    app::hint(
        ui,
        "最多 28 个房主同时开房；加入不占名额，无需凭证和 frpc，人数由 Minecraft 自身决定。",
    );
    ui.add_space(18.0);
    let busy = app.multiplayer.pending.is_some() || app.multiplayer.session.is_some();
    theme::card(18).show(ui, |ui| {
        ui.set_width((ui.available_width() - 4.0).max(0.0));
        ui.spacing_mut().item_spacing.y = 10.0;
        app::body(ui, &account.as_ref().map(|a| format!("当前账号：{}", a.name)).unwrap_or_else(|| "创建房间需选择账号；加入好友房间无需选择。".into()));
        ui.add_enabled_ui(!busy, |ui| {
            ui.label("验证服务地址（自建服务器）");
            ui.add(egui::TextEdit::singleline(&mut app.settings.multiplayer_endpoint)
                .hint_text("例如 https://auth.example.com")
                .desired_width(ui.available_width()));
            app::hint(ui,"填写部署在你服务器上的验证服务 HTTPS 地址。88frp 的节点 IP、域名和端口由映射池自动分配。");
            if download_button(ui,"测试验证服务",true) {
                app.multiplayer.check_service(app.settings.multiplayer_endpoint.clone(),ui.ctx());
            }
            ui.label("房主凭证（仅创建房间需要）");
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
                if download_button(ui,"自动下载 frpc",true) {app.multiplayer.download(ui.ctx());}
            });
            theme::toolbar(ui, |ui| {
                if download_button(ui, "保存联机设置", true) {
                    app.multiplayer.message = match crate::store::save(&app.settings) {
                        Ok(()) => "联机设置已保存。".into(), Err(error) => error,
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
            app::body(
                ui,
                if session.hosting() {
                    "我的房间"
                } else {
                    "已加入房间"
                },
            );
            app::body(ui, &format!("房间码：{}", session.room()));
            {
                let frpc = &session.0;
                let latency = frpc
                    .latency_ms
                    .map(|n| format!(" · TCP 延迟 {n} ms"))
                    .unwrap_or_default();
                app::body(ui, &format!("节点：{}{latency}", frpc.node_name));
                app::hint(
                    ui,
                    &format!("隧道地址：{}（好友请通过房间码加入）", frpc.public_address),
                );
            }
            if let Some(address) = session.address() {
                app::body(ui, &format!("游戏连接地址：{address}"));
                app::hint(
                    ui,
                    "在 Minecraft「多人游戏 → 直接连接」中输入此地址，并保持启动器开启。",
                );
            } else {
                app::hint(
                    ui,
                    "将房间码分享给好友，对方无需房主凭证即可加入。房主需保持游戏和启动器开启。",
                );
            }
            app::hint(ui, &format!("当前数据连接：{}", session.connections()));
            let message = session.message();
            if !message.is_empty() {
                app::hint(ui, &message);
            }
            if !session.running() {
                app::hint(ui, "联机已停止，请关闭后重新连接。");
            }
            theme::toolbar(ui, |ui| {
                if download_button(ui, "复制房间码", true) {
                    ui.ctx().copy_text(session.room().to_owned());
                }
                if let Some(address) = session.address()
                    && download_button(ui, "复制连接地址", true)
                {
                    ui.ctx().copy_text(address.to_string());
                }
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
                    app.multiplayer.start(app.settings.multiplayer_endpoint.clone(), account.clone(), true,app.settings.frpc_path.clone(), ui.ctx());
                }
                ui.add_space(14.0);
            });
            ui.add_enabled_ui(!busy,|ui| {
                app::body(ui, "加入房间");
                ui.add(egui::TextEdit::singleline(&mut app.multiplayer.room_code).hint_text("粘贴朋友分享的房间码").desired_width(ui.available_width()));
                if download_button(ui, "加入联机房间", true) {
                    app.multiplayer.start(app.settings.multiplayer_endpoint.clone(), None, false,String::new(), ui.ctx());
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
    if app.multiplayer.download_prompt {
        let mut open = true;
        egui::Window::new("需要下载 frpc")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ui.ctx(), |ui| {
                app::body(
                    ui,
                    "创建房间需要 frpc。启动器可以从 FRP 官方下载并校验后安装。",
                );
                app::hint(ui, "仅加入好友房间不需要下载，也不占 28 个房主名额。");
                ui.add_space(12.0);
                if ui
                    .add_enabled_ui(!busy, |ui| app::primary_button(ui, "下载 frpc"))
                    .inner
                {
                    app.multiplayer.download(ui.ctx());
                }
                if download_button(ui, "仅加入房间", true) {
                    app.multiplayer.download_prompt = false;
                }
                if download_button(ui, "选择已有 frpc", cfg!(windows) && !busy) {
                    match crate::file_picker::frpc() {
                        Ok(Some(path)) => {
                            app.multiplayer.ready_program = Some(path);
                            app.multiplayer.download_prompt = false;
                        }
                        Ok(None) => {}
                        Err(error) => app.multiplayer.message = error,
                    }
                }
            });
        if !open {
            app.multiplayer.download_prompt = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Page;

    #[test]
    fn missing_frpc_prompts_on_entry_and_existing_program_skips_download() {
        let (root, _, _) = crate::service::tests::fixture();
        let mut page = MultiplayerPage::default();
        page.check_program("", root.path());
        assert!(page.download_prompt);
        page.download_prompt = false;
        page.check_program("", root.path());
        assert!(!page.download_prompt);
        page.leave();
        page.check_program("", root.path());
        assert!(page.download_prompt);
        let existing = std::env::current_exe().unwrap();
        page.leave();
        page.check_program(existing.to_str().unwrap(), root.path());
        assert!(!page.download_prompt);
    }

    #[test]
    fn joining_needs_neither_selected_account_host_token_nor_frpc() {
        let (_root, _, ctx) = crate::service::tests::fixture();
        let mut page = MultiplayerPage::default();
        page.start(
            "http://127.0.0.1:1".into(),
            None,
            false,
            String::new(),
            &ctx,
        );
        assert!(page.pending.is_some());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while page.pending.is_some() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
            page.poll(None);
        }
        assert!(page.message.contains("房间码"), "{}", page.message);
        assert!(page.token.is_empty());
        assert!(page.ready_program.is_none());
    }

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
            "验证服务地址（自建服务器）",
            "测试验证服务",
            "房主凭证（仅创建房间需要）",
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
