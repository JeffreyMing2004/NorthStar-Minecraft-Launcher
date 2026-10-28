use crate::{
    app::{self, NsclApp},
    pages::download_button,
    theme,
};
use nscl::multiplayer::frpc::download as frpc_download;
use nscl::multiplayer::frpc::{FrpcSession, GAME_PORT, HostOptions, VERIFICATION_ENDPOINTS};
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
}
#[derive(Clone, Copy)]
enum Task {
    Download,
    Host,
    Join,
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
    room_code: String,
    message: String,
    pending: Option<Receiver<Completion>>,
    task: Option<Task>,
    pending_cancel: Option<Arc<AtomicBool>>,
    progress: Arc<Mutex<String>>,
    session: Option<Session>,
    entered: bool,
    download_prompt: bool,
    ready_program: Option<PathBuf>,
    program: Option<PathBuf>,
    detection: Option<Receiver<Option<PathBuf>>>,
    detected: bool,
}

impl MultiplayerPage {
    pub fn poll(&mut self, _account: Option<&Account>) {
        if let Some(receiver) = &self.pending {
            match receiver.try_recv() {
                Ok(completion) => {
                    self.pending = None;
                    self.task = None;
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
                        Completion::Downloaded(result) => {
                            match result {
                                Ok(path) => {
                                    self.accept_program(path);
                                    self.message = "frpc 已下载并校验完成，可以创建房间。".into();
                                }
                                Err(error) => self.message = error,
                            }
                            return;
                        }
                        Completion::Connected { result, program } => {
                            if let Some(program) = program {
                                self.accept_program(program);
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
                    self.task = None;
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
    fn accept_program(&mut self, path: PathBuf) {
        self.program = Some(path.clone());
        self.ready_program = Some(path);
        self.detected = true;
        self.detection = None;
        self.download_prompt = false;
    }

    pub fn detect_program(&mut self, configured: &str, directory: &Path, ctx: &egui::Context) {
        if !self.detected && self.detection.is_none() {
            let (sender, receiver) = mpsc::channel();
            self.detection = Some(receiver);
            let (configured, directory, ctx) =
                (configured.to_owned(), directory.to_owned(), ctx.clone());
            std::thread::spawn(move || {
                let program = find_program(&configured, &directory);
                let _ = sender.send(program);
                ctx.request_repaint();
            });
        }
        if let Some(receiver) = &self.detection {
            match receiver.try_recv() {
                Ok(program) => {
                    self.detection = None;
                    self.detected = true;
                    if let Some(program) = program {
                        self.accept_program(program);
                    } else if self.entered {
                        self.download_prompt = true;
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.detection = None;
                    self.detected = true;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.detection.is_some() || self.pending.is_some() || self.session.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }

    pub fn take_program(&mut self) -> Option<PathBuf> {
        self.ready_program.take()
    }

    pub fn status(&self) -> &'static str {
        if let Some(session) = &self.session {
            return match (session.hosting(), session.running()) {
                (true, true) => "frpc 运行中",
                (true, false) => "frpc 已停止",
                (false, true) => "联机已连接",
                (false, false) => "联机已断开",
            };
        }
        match self.task {
            Some(Task::Download) => "frpc 下载中",
            Some(Task::Host) => "frpc 启动中",
            Some(Task::Join) => "正在加入联机",
            None if !self.detected => "frpc 检测中",
            None if self.program.as_ref().is_some_and(|p| p.is_file()) => "frpc 已就绪",
            None => "frpc 未安装",
        }
    }

    fn enter(&mut self) {
        if !self.entered {
            self.entered = true;
            self.download_prompt =
                self.detected && !self.program.as_ref().is_some_and(|p| p.is_file());
        }
    }
    fn download(&mut self, ctx: &egui::Context) {
        if self.pending.is_some() || self.session.is_some() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.pending = Some(receiver);
        self.task = Some(Task::Download);
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

    fn start(
        &mut self,
        endpoints: &[&str],
        hosting: bool,
        executable: String,
        ctx: &egui::Context,
    ) {
        if self.pending.is_some() || self.session.is_some() {
            return;
        }
        let port = GAME_PORT;
        let (sender, receiver) = mpsc::channel();
        self.pending = Some(receiver);
        self.task = Some(if hosting { Task::Host } else { Task::Join });
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
        let endpoints: Vec<String> = endpoints.iter().map(|s| (*s).into()).collect();
        std::thread::spawn(move || {
            let mut program = None;
            let result = (|| {
                if !hosting && !room.trim().starts_with("frpc:") {
                    return Err("请填写好友分享的 frpc 房间码。".into());
                }
                let endpoints: Vec<&str> = endpoints.iter().map(String::as_str).collect();
                let client = RelayClient::available(&endpoints)?;
                if cancel.load(Ordering::Relaxed) {
                    return Err("操作已取消。".into());
                }
                if hosting {
                    let report_ctx = ctx.clone();
                    let report: Arc<dyn Fn(String) + Send + Sync> = Arc::new(move |message| {
                        *progress.lock().unwrap() = message;
                        report_ctx.request_repaint();
                    });
                    let selected = PathBuf::from(executable.trim().trim_matches('"'));
                    let executable = if selected.is_file() {
                        Ok(selected)
                    } else {
                        frpc_download::install(
                            &frpc_download::directory(),
                            &cancel,
                            report.as_ref(),
                        )
                    };
                    executable.and_then(|executable| {
                        program = Some(executable.clone());
                        client
                            .host_frpc_room(
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
                } else {
                    client.join_frpc_room(&room, cancel).map(Session)
                }
            })();
            let _ = sender.send(Completion::Connected { result, program });
            ctx.request_repaint();
        });
    }
}

fn find_program(configured: &str, directory: &Path) -> Option<PathBuf> {
    let configured = PathBuf::from(configured.trim().trim_matches('"'));
    if configured.is_file() {
        return Some(configured);
    }
    if let Some(program) = frpc_download::installed(directory) {
        return Some(program);
    }
    let name = if cfg!(windows) { "frpc.exe" } else { "frpc" };
    std::iter::once(crate::store::launcher_directory())
        .chain(
            std::env::var_os("PATH")
                .into_iter()
                .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>()),
        )
        .map(|p| p.join(name))
        .find(|p| p.is_file())
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
    app.multiplayer.enter();
    app::heading(ui, "联机");
    ui.add_space(18.0);
    let busy = app.multiplayer.pending.is_some() || app.multiplayer.session.is_some();
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
                app::hint(ui, &format!("Minecraft 直连地址：{}", frpc.public_address));
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
                    "分享直连地址或房间码即可加入。房主需保持游戏和启动器开启。",
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
                if download_button(ui, "复制直连地址", true) {
                    ui.ctx().copy_text(session.0.public_address.clone());
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
            ui.add_enabled_ui(!busy, |ui| {
                app::body(ui, "创建房间");
                app::hint(ui, "只能使用 25565 端口开房。请将游戏「对局域网开放」的端口或服务端端口设为 25565，再创建房间。好友需使用兼容的游戏版本和模组。");
                if app::primary_button(ui, "创建联机房间") {
                    if !app.multiplayer.detected {
                        app.multiplayer.message = "正在检测 frpc，请稍候。".into();
                    } else if let Some(program) = app.multiplayer.program.as_ref().filter(|p| p.is_file()) {
                        let executable = program.to_string_lossy().into_owned();
                        app.multiplayer.start(&VERIFICATION_ENDPOINTS, true, executable, ui.ctx());
                    } else {
                        app.multiplayer.download_prompt = true;
                    }
                }
                ui.add_space(14.0);
            });
            ui.add_enabled_ui(!busy,|ui| {
                app::body(ui, "加入房间");
                ui.add(egui::TextEdit::singleline(&mut app.multiplayer.room_code).hint_text("粘贴朋友分享的房间码").desired_width(ui.available_width()));
                if download_button(ui, "加入联机房间", true) {
                    app.multiplayer.start(&VERIFICATION_ENDPOINTS, false,String::new(), ui.ctx());
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
                            app.multiplayer.accept_program(path);
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
        let (root, _, ctx) = crate::service::tests::fixture();
        let mut page = MultiplayerPage::default();
        let (sender, receiver) = mpsc::channel();
        page.detection = Some(receiver);
        sender.send(None).unwrap();
        page.detect_program("", root.path(), &ctx);
        page.enter();
        assert!(page.download_prompt);
        assert_eq!(page.status(), "frpc 未安装");
        page.download_prompt = false;
        page.enter();
        assert!(!page.download_prompt);
        page.leave();
        page.enter();
        assert!(page.download_prompt);
        let existing = std::env::current_exe().unwrap();
        page = MultiplayerPage::default();
        page.detect_program(existing.to_str().unwrap(), root.path(), &ctx);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !page.detected && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
            page.detect_program(existing.to_str().unwrap(), root.path(), &ctx);
        }
        assert_eq!(page.take_program(), Some(existing));
        assert_eq!(page.status(), "frpc 已就绪");
        page.leave();
        page.enter();
        assert!(!page.download_prompt);
    }

    #[test]
    fn joining_needs_neither_selected_account_host_token_nor_frpc() {
        let (_root, _, ctx) = crate::service::tests::fixture();
        let mut page = MultiplayerPage::default();
        page.start(&["http://127.0.0.1:1"], false, String::new(), &ctx);
        assert!(page.pending.is_some());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while page.pending.is_some() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
            page.poll(None);
        }
        assert!(page.message.contains("房间码"), "{}", page.message);
        assert!(page.ready_program.is_none());
    }

    #[test]
    fn launcher_account_changes_do_not_interrupt_anonymous_multiplayer() {
        assert!(Page::ALL.contains(&Page::Multiplayer));
        let alice = Account::offline("Alice").unwrap();
        let bob = Account::offline("Bob").unwrap();
        let mut page = MultiplayerPage::default();
        page.poll(Some(&alice));
        page.message = "old account".into();
        page.poll(Some(&alice));
        page.poll(Some(&bob));
        assert_eq!(page.message, "old account");
        page.poll(None);
        assert_eq!(page.message, "old account");
    }

    #[test]
    fn hosting_starts_without_a_code_or_selected_account() {
        let (_root, _, ctx) = crate::service::tests::fixture();
        let mut page = MultiplayerPage::default();
        page.start(
            &["http://127.0.0.1:1"],
            true,
            std::env::current_exe()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            &ctx,
        );
        assert!(page.pending.is_some());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
        while page.pending.is_some() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
            page.poll(None);
        }
        assert!(page.pending.is_none());
        assert!(!page.message.contains("开房码"));
    }

    #[test]
    fn multiplayer_page_renders_in_the_minimum_window_and_does_not_persist_credentials() {
        let (_root, mut app, ctx) = crate::service::tests::fixture();
        app.page = Page::Multiplayer;
        let alice = Account::offline("Alice").unwrap();
        app.accounts.list.accounts = vec![alice.clone()];
        app.accounts.list.selected = Some(alice.id);
        app.multiplayer.poll(app.accounts.list.current());
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
        for label in ["联机", "创建联机房间", "加入联机房间"] {
            crate::service::tests::text_rect(&output, label);
        }
        let labels = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        for removed in [
            "联机服务地址",
            "保存联机设置",
            "88frp 帮助",
            "房主 frpc 程序设置",
            "主验证地址",
            "auth.minguncle.cn",
            "房主通过 frpc",
        ] {
            assert!(!labels.contains(removed), "unexpected setting: {removed}");
        }
        assert!(labels.contains("只能使用 25565 端口开房"));
        let settings = serde_json::to_string(&app.settings).unwrap();
        assert!(!settings.contains("owner_token"));
        assert!(!settings.contains("multiplayer_endpoint"));
        output.drop_without_applying_deltas();
    }
}
