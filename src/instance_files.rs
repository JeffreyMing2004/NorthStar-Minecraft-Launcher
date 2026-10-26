use crate::{app, instances::Instance, pages::download_button, theme};
use egui::{RichText, Ui, Vec2};
use std::{
    ffi::OsString,
    fs,
    path::{Component, Path, PathBuf},
    sync::mpsc::{self, Receiver, TryRecvError},
};

const MAX_ENTRIES: usize = 1000;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Folder {
    #[default]
    Game,
    Saves,
    Mods,
    Resources,
    Shaders,
    Screenshots,
    Logs,
}

impl Folder {
    const ALL: [Self; 7] = [
        Self::Game,
        Self::Saves,
        Self::Mods,
        Self::Resources,
        Self::Shaders,
        Self::Screenshots,
        Self::Logs,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Game => "游戏目录",
            Self::Saves => "存档",
            Self::Mods => "模组",
            Self::Resources => "资源包",
            Self::Shaders => "光影",
            Self::Screenshots => "截图",
            Self::Logs => "日志",
        }
    }

    fn directory(self) -> &'static str {
        match self {
            Self::Game => "",
            Self::Saves => "saves",
            Self::Mods => "mods",
            Self::Resources => "resourcepacks",
            Self::Shaders => "shaderpacks",
            Self::Screenshots => "screenshots",
            Self::Logs => "logs",
        }
    }
}

struct Entry {
    name: OsString,
    directory: bool,
    link: bool,
    size: u64,
}

struct Listing {
    exists: bool,
    entries: Vec<Entry>,
    truncated: bool,
}

type ReadResult = Result<Listing, String>;

#[derive(Default)]
pub struct Browser {
    folder: Folder,
    base: Option<PathBuf>,
    relative: PathBuf,
    selected: Option<OsString>,
    data: Option<(PathBuf, ReadResult)>,
    pending: Option<(PathBuf, Receiver<ReadResult>)>,
    notice: String,
}

impl Browser {
    pub fn select(&mut self, folder: Folder, name: Option<OsString>) {
        self.folder = folder;
        self.base = None;
        self.relative.clear();
        self.selected = name;
        self.data = None;
        self.notice.clear();
    }

    fn request(&mut self, ctx: &egui::Context, path: &Path) {
        if let Some((pending_path, receiver)) = &self.pending {
            let result = match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => {
                    Some(Err("读取目录的任务已结束，请刷新重试。".into()))
                }
            };
            if let Some(result) = result {
                self.data = Some((pending_path.clone(), result));
                self.pending = None;
            }
        }
        if self.pending.is_some() || self.data.as_ref().is_some_and(|(loaded, _)| loaded == path) {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        let (directory, context) = (path.to_path_buf(), ctx.clone());
        match std::thread::Builder::new()
            .name("instance-files".into())
            .spawn(move || {
                let _ = sender.send(read_directory(&directory));
                context.request_repaint();
            }) {
            Ok(_) => self.pending = Some((path.to_path_buf(), receiver)),
            Err(error) => {
                self.data = Some((path.to_path_buf(), Err(format!("无法读取目录：{error}"))))
            }
        }
    }
}

fn read_directory(path: &Path) -> ReadResult {
    let source = match fs::read_dir(path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Listing {
                exists: false,
                entries: vec![],
                truncated: false,
            });
        }
        Err(error) => return Err(format!("无法读取此目录：{error}")),
    };
    let mut entries = Vec::new();
    let mut truncated = false;
    for item in source {
        if entries.len() == MAX_ENTRIES {
            truncated = true;
            break;
        }
        let item = item.map_err(|error| format!("无法读取目录项：{error}"))?;
        let metadata = fs::symlink_metadata(item.path())
            .map_err(|error| format!("无法读取 {}：{error}", item.file_name().to_string_lossy()))?;
        let link = metadata.file_type().is_symlink();
        #[cfg(windows)]
        let link = {
            use std::os::windows::fs::MetadataExt;
            link || metadata.file_attributes() & 0x400 != 0
        };
        entries.push(Entry {
            name: item.file_name(),
            directory: metadata.is_dir(),
            link,
            size: metadata.len(),
        });
    }
    entries.sort_by_key(|entry| {
        (
            !entry.directory,
            entry.name.to_string_lossy().to_lowercase(),
        )
    });
    Ok(Listing {
        exists: true,
        entries,
        truncated,
    })
}

fn size_label(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / 1024.0 / 1024.0)
    } else {
        format!("{:.1} GiB", bytes as f64 / 1024.0 / 1024.0 / 1024.0)
    }
}

pub fn show(ui: &mut Ui, root: &Path, instance: &Instance, browser: &mut Browser) {
    let game = match instance.game_directory(root) {
        Ok(game) => game,
        Err(error) => {
            app::body(ui, &error);
            return;
        }
    };
    if browser.base.as_ref() != Some(&game) {
        if browser.base.is_some() {
            browser.selected = None;
            browser.notice.clear();
        }
        browser.base = Some(game.clone());
        browser.relative.clear();
        browser.data = None;
    }
    let columns = ((ui.available_width() + theme::CONTROL_GAP) / (104.0 + theme::CONTROL_GAP))
        .floor()
        .max(1.0) as usize;
    for row in Folder::ALL.chunks(columns) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme::CONTROL_GAP;
            for &folder in row {
                if app::tab_button(ui, folder.label(), browser.folder == folder) {
                    browser.select(folder, None);
                }
            }
        });
        ui.add_space(theme::CONTROL_GAP);
    }
    // Only directory names returned by read_dir may extend the relative path.
    debug_assert!(
        browser
            .relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    );
    let base = game.join(browser.folder.directory());
    let path = base.join(&browser.relative);
    ui.add(
        egui::Label::new(
            RichText::new(path.to_string_lossy())
                .size(11.0)
                .color(theme::HINT),
        )
        .truncate(),
    )
    .on_hover_text(path.to_string_lossy());
    theme::toolbar(ui, |ui| {
        if download_button(ui, "上一级", !browser.relative.as_os_str().is_empty()) {
            browser.relative.pop();
            browser.selected = None;
            browser.data = None;
            ui.ctx().request_repaint();
        }
        if download_button(ui, "打开目录", true) {
            browser.notice = match fs::create_dir_all(&path)
                .map_err(|e| e.to_string())
                .and_then(|_| open::that_detached(&path).map_err(|e| e.to_string()))
            {
                Ok(()) => "已请求在资源管理器中打开此目录".into(),
                Err(error) => format!("打开目录失败：{error}"),
            };
        }
        if download_button(ui, "复制路径", true) {
            ui.ctx().copy_text(path.to_string_lossy().into_owned());
            browser.notice = "路径已复制".into();
        }
        if download_button(ui, "刷新", true) {
            browser.data = None;
            browser.notice.clear();
        }
    });
    if !browser.notice.is_empty() {
        app::hint(ui, &browser.notice);
    }
    browser.request(ui.ctx(), &path);
    let Some((loaded, result)) = &browser.data else {
        app::hint(ui, "正在读取目录……");
        return;
    };
    if loaded != &path {
        app::hint(ui, "正在读取目录……");
        return;
    }
    let listing = match result {
        Ok(listing) => listing,
        Err(error) => {
            app::body(ui, error);
            return;
        }
    };
    if listing.entries.is_empty() {
        app::body(
            ui,
            if listing.exists {
                "此目录为空"
            } else {
                "此目录尚未创建"
            },
        );
        app::hint(
            ui,
            match browser.folder {
                Folder::Saves => "创建并保存游戏世界后，存档会出现在这里。",
                Folder::Mods => "此实例尚无模组文件，可从下载页安装或打开目录手动添加。",
                Folder::Resources => "此实例尚无资源包，可打开目录放入资源包 ZIP。",
                Folder::Shaders => "此实例尚无光影包，可打开目录放入光影 ZIP。",
                Folder::Screenshots => "在游戏中按 F2 截图后，图片会出现在这里。",
                Folder::Logs => "运行游戏后，日志会出现在这里。",
                Folder::Game => "启动游戏后会生成配置、存档等文件。",
            },
        );
        return;
    }
    app::hint(
        ui,
        &format!(
            "{} 项{}",
            listing.entries.len(),
            if listing.truncated {
                " · 仅显示前 1000 项"
            } else {
                ""
            }
        ),
    );
    if let Some(entry) = listing
        .entries
        .iter()
        .find(|e| browser.selected.as_ref() == Some(&e.name))
    {
        ui.separator();
        ui.add(
            egui::Label::new(RichText::new(entry.name.to_string_lossy()).color(theme::HEADING))
                .truncate(),
        );
        app::hint(
            ui,
            &format!(
                "{} · {}",
                if entry.link { "链接" } else { "文件" },
                size_label(entry.size)
            ),
        );
        ui.add(egui::Label::new(path.join(&entry.name).to_string_lossy()).wrap());
    }
    let mut enter = None;
    egui::ScrollArea::vertical()
        .id_salt(("instance-files", &path))
        .max_height(180.0)
        .min_scrolled_height(30.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.spacing_mut().interact_size.y = 28.0;
            ui.spacing_mut().button_padding = Vec2::new(6.0, 4.0);
            ui.spacing_mut().item_spacing.y = 2.0;
            for entry in &listing.entries {
                ui.horizontal(|ui| {
                    let label = if entry.link {
                        "链接".into()
                    } else if entry.directory {
                        "文件夹".into()
                    } else {
                        size_label(entry.size)
                    };
                    ui.label(RichText::new(label).size(11.0).color(theme::HINT));
                    let selected = browser.selected.as_ref() == Some(&entry.name);
                    if ui
                        .add(
                            egui::Button::selectable(selected, entry.name.to_string_lossy())
                                .truncate(),
                        )
                        .clicked()
                    {
                        if entry.directory && !entry.link {
                            enter = Some(entry.name.clone());
                        } else {
                            browser.selected = Some(entry.name.clone());
                        }
                    }
                });
            }
        });
    if let Some(name) = enter {
        browser.relative.push(name);
        browser.selected = None;
        browser.data = None;
        browser.notice.clear();
        ui.ctx().request_repaint();
    }
}

#[cfg(test)]
mod tests;
