use crate::{
    account_service::AccountService,
    app::{self, NsclApp},
    theme,
};
use egui::{Color32, Pos2, Rect, RichText, Sense, TextureHandle, TextureOptions, Ui, Vec2};
use nscl::{
    accounts::{AuthClient, Session, SystemVault},
    skins::{self, Model, Skin},
};
use std::{
    path::PathBuf,
    sync::{
        atomic::AtomicBool,
        mpsc::{self, Receiver, Sender},
    },
};

type Loaded = (u64, Result<(Skin, Option<Session>, String), String>);
pub struct SkinView {
    image: Skin,
    texture: Option<TextureHandle>,
    pub path: String,
    pub model: Model,
    pub status: String,
    pub pending: bool,
    pub local: bool,
    account: Option<String>,
    generation: u64,
    sender: Sender<Loaded>,
    receiver: Receiver<Loaded>,
    yaw: f32,
    pitch: f32,
}

impl SkinView {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            image: skins::default_skin(),
            texture: None,
            path: String::new(),
            model: Model::Classic,
            status: "默认 Steve；登录后自动显示账号皮肤，也可预览本地 PNG。".into(),
            pending: false,
            local: false,
            account: None,
            generation: 0,
            sender,
            receiver,
            yaw: -0.4,
            pitch: 0.12,
        }
    }
    pub fn sync(&mut self, accounts: &mut AccountService, online: bool, ctx: &egui::Context) {
        let selected = accounts
            .list
            .current()
            .filter(|_| online)
            .map(|a| a.id.clone());
        if selected != self.account {
            self.account = selected;
            self.generation += 1;
            self.pending = false;
            self.local = false;
            self.image = skins::default_skin();
            self.texture = None;
            self.model = Model::Classic;
            self.status = "默认 Steve".into();
            if self.account.is_some() {
                self.remote(accounts, false, ctx);
            }
        }
        while let Ok((generation, result)) = self.receiver.try_recv() {
            if generation != self.generation {
                continue;
            }
            self.pending = false;
            match result {
                Ok((skin, session, status)) => {
                    self.image = skin;
                    self.model = self.image.model;
                    self.texture = None;
                    self.status = status;
                    if let Some(session) = session {
                        accounts.accept(session);
                    }
                }
                Err(error) => self.status = error,
            }
        }
    }
    fn preview(&mut self, ctx: &egui::Context) {
        if self.pending {
            return;
        }
        self.pending = true;
        self.local = true;
        let (path, model, generation, sender, ctx) = (
            PathBuf::from(self.path.trim().trim_matches('"')),
            self.model,
            self.generation,
            self.sender.clone(),
            ctx.clone(),
        );
        std::thread::spawn(move || {
            let result = skins::read(&path, model).map(|(_, image)| {
                (
                    image,
                    None,
                    "本地预览已加载；点击上传才会修改账号皮肤。".into(),
                )
            });
            let _ = sender.send((generation, result));
            ctx.request_repaint();
        });
    }
    fn remote(&mut self, accounts: &AccountService, upload: bool, ctx: &egui::Context) {
        if self.pending {
            return;
        }
        let Some((account, cached)) = accounts.launch_account() else {
            self.status = "请先登录 Microsoft 正版账号。".into();
            return;
        };
        self.pending = true;
        self.local = upload;
        self.status = if upload {
            "正在上传皮肤……"
        } else {
            "正在读取账号皮肤……"
        }
        .into();
        let (path, model, generation, sender, ctx) = (
            PathBuf::from(self.path.trim().trim_matches('"')),
            self.model,
            self.generation,
            self.sender.clone(),
            ctx.clone(),
        );
        std::thread::spawn(move || {
            let cache = nscl::store::settings_path()
                .with_file_name("cache")
                .join("skins")
                .join(format!("{}.png", account.id));
            let result = (|| {
                let session = AuthClient::new()?.resume(
                    &account,
                    cached,
                    &SystemVault,
                    &AtomicBool::new(false),
                )?;
                let client = skins::SkinClient::new()?;
                let (image, status) = if upload {
                    let (bytes, image) = skins::read(&path, model)?;
                    client.upload(&session, bytes, model)?;
                    (
                        image,
                        "皮肤已上传，游戏内显示可能需要重新进入服务器。".into(),
                    )
                } else {
                    (client.current(&session)?, "账号皮肤已刷新。".into())
                };
                if let Some(parent) = cache.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = image.pixels.save(&cache);
                // Save model separately; skin PNG does not encode classic/slim metadata.
                let _ = std::fs::write(cache.with_extension("model"), image.model.key());
                Ok((image, Some(session), status))
            })();
            let result = result.or_else(|error: String| {
                if !upload {
                    let model = if std::fs::read_to_string(cache.with_extension("model"))
                        .ok()
                        .as_deref()
                        == Some("slim")
                    {
                        Model::Slim
                    } else {
                        Model::Classic
                    };
                    if let Ok((_, image)) = skins::read(&cache, model) {
                        return Ok((image, None, format!("{error} 正在显示缓存皮肤。")));
                    }
                }
                Err(error)
            });
            let _ = sender.send((generation, result));
            ctx.request_repaint();
        });
    }
    pub fn paint(&mut self, ui: &mut Ui, rect: Rect, id: &'static str) {
        let response = ui.interact(rect, ui.make_persistent_id(id), Sense::click_and_drag());
        if response.dragged() {
            let delta = ui.input(|i| i.pointer.delta());
            self.yaw += delta.x * 0.015;
            self.pitch = (self.pitch + delta.y * 0.01).clamp(-0.65, 0.65);
        }
        if response.double_clicked() {
            self.yaw = -0.4;
            self.pitch = 0.12;
        }
        let texture = self.texture.get_or_insert_with(|| {
            ui.ctx().load_texture(
                "minecraft-skin",
                egui::ColorImage::from_rgba_unmultiplied([64, 64], self.image.pixels.as_raw()),
                TextureOptions::NEAREST,
            )
        });
        let painter = ui.painter().with_clip_rect(rect);
        let scale = (rect.height() / 38.0).min(rect.width() / 25.0).max(0.0);
        let center = rect.center() + Vec2::new(0.0, -4.0);
        painter.circle_filled(
            center + Vec2::new(0.0, 17.0 * scale),
            5.0 * scale,
            Color32::from_black_alpha(28),
        );
        let arm = if self.image.model == Model::Slim {
            3.0
        } else {
            4.0
        };
        let mut faces = Vec::new();
        for (position, size, uv, overlay) in [
            ([0., 12., 0.], [8., 8., 8.], [0., 0.], false),
            ([0., 2., 0.], [8., 12., 4.], [16., 16.], false),
            ([-4. - arm / 2., 2., 0.], [arm, 12., 4.], [40., 16.], false),
            ([4. + arm / 2., 2., 0.], [arm, 12., 4.], [32., 48.], false),
            ([-2., -10., 0.], [4., 12., 4.], [0., 16.], false),
            ([2., -10., 0.], [4., 12., 4.], [16., 48.], false),
            ([0., 12., 0.], [8., 8., 8.], [32., 0.], true),
            ([0., 2., 0.], [8., 12., 4.], [16., 32.], true),
            ([-4. - arm / 2., 2., 0.], [arm, 12., 4.], [40., 32.], true),
            ([4. + arm / 2., 2., 0.], [arm, 12., 4.], [48., 48.], true),
            ([-2., -10., 0.], [4., 12., 4.], [0., 32.], true),
            ([2., -10., 0.], [4., 12., 4.], [0., 48.], true),
        ] {
            cube(
                &mut faces,
                position,
                size,
                uv,
                if overlay { 0.25 } else { 0. },
                self.yaw,
                self.pitch,
            );
        }
        faces.sort_by(|a, b| a.depth.total_cmp(&b.depth));
        for face in faces {
            let mut mesh = egui::Mesh::with_texture(texture.id());
            for (point, uv) in face.points.into_iter().zip(face.uvs) {
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: center + Vec2::new(point[0] * scale, -point[1] * scale),
                    uv: Pos2::new(uv[0] / 64., uv[1] / 64.),
                    color: Color32::from_gray(face.light),
                });
            }
            mesh.indices.extend([0, 1, 2, 0, 2, 3]);
            painter.add(egui::Shape::mesh(mesh));
        }
        painter.text(
            Pos2::new(rect.center().x, rect.bottom() - 9.0),
            egui::Align2::CENTER_BOTTOM,
            "拖动旋转 · 双击复位",
            egui::FontId::proportional(11.0),
            theme::HINT,
        );
    }
}

struct Face {
    points: [[f32; 3]; 4],
    uvs: [[f32; 2]; 4],
    depth: f32,
    light: u8,
}
fn rotate([x, y, z]: [f32; 3], yaw: f32, pitch: f32) -> [f32; 3] {
    let (s, c) = yaw.sin_cos();
    let (x, z) = (x * c + z * s, -x * s + z * c);
    let (s, c) = pitch.sin_cos();
    [x, y * c - z * s, y * s + z * c]
}
fn cube(
    faces: &mut Vec<Face>,
    position: [f32; 3],
    size: [f32; 3],
    origin: [f32; 2],
    inflate: f32,
    yaw: f32,
    pitch: f32,
) {
    let [w, h, d] = size;
    let (x, y, z) = (w / 2. + inflate, h / 2. + inflate, d / 2. + inflate);
    let [u, v] = origin;
    for (vertices, uv, normal) in [
        (
            [[-x, y, z], [x, y, z], [x, -y, z], [-x, -y, z]],
            [u + d, v + d, w, h],
            [0., 0., 1.],
        ),
        (
            [[x, y, -z], [-x, y, -z], [-x, -y, -z], [x, -y, -z]],
            [u + 2. * d + w, v + d, w, h],
            [0., 0., -1.],
        ),
        (
            [[-x, y, -z], [-x, y, z], [-x, -y, z], [-x, -y, -z]],
            [u, v + d, d, h],
            [-1., 0., 0.],
        ),
        (
            [[x, y, z], [x, y, -z], [x, -y, -z], [x, -y, z]],
            [u + d + w, v + d, d, h],
            [1., 0., 0.],
        ),
        (
            [[-x, y, -z], [x, y, -z], [x, y, z], [-x, y, z]],
            [u + d, v, w, d],
            [0., 1., 0.],
        ),
        (
            [[-x, -y, z], [x, -y, z], [x, -y, -z], [-x, -y, -z]],
            [u + d + w, v, w, d],
            [0., -1., 0.],
        ),
    ] {
        let n = rotate(normal, yaw, pitch);
        if n[2] <= 0. {
            continue;
        }
        let points = vertices.map(|p| {
            rotate(
                [p[0] + position[0], p[1] + position[1], p[2] + position[2]],
                yaw,
                pitch,
            )
        });
        let [u, v, w, h] = uv;
        faces.push(Face {
            depth: points.iter().map(|p| p[2]).sum::<f32>() / 4.,
            points,
            uvs: [[u, v], [u + w, v], [u + w, v + h], [u, v + h]],
            light: (210. + 35. * n[1] + 10. * n[2]).clamp(150., 255.) as u8,
        });
    }
}

pub fn controls(ui: &mut Ui, app: &mut NsclApp) {
    let width = ui.available_width();
    if width >= 640.0 {
        let height = (ui.ctx().content_rect().height() - 346.0).clamp(270.0, 330.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        let model_width = width * 0.36;
        let model_rect = Rect::from_min_size(rect.min, Vec2::new(model_width, rect.height()));
        ui.painter().rect_filled(model_rect, 12.0, theme::TOPBAR_BG);
        app.skin
            .paint(ui, model_rect.shrink(12.0), "account-skin-model");
        let form_rect = Rect::from_min_max(
            Pos2::new(model_rect.right() + 24.0, rect.top() + 8.0),
            rect.max,
        );
        let mut form = ui.new_child(egui::UiBuilder::new().max_rect(form_rect));
        form.set_clip_rect(form_rect.intersect(ui.clip_rect()));
        form_controls(&mut form, app);
    } else {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 230.0), Sense::hover());
        app.skin.paint(ui, rect, "account-skin-model");
        ui.add_space(12.0);
        form_controls(ui, app);
    }
}

fn form_controls(ui: &mut Ui, app: &mut NsclApp) {
    let width = ui.available_width();
    ui.label(
        RichText::new(
            app.accounts
                .list
                .current()
                .map(|a| a.name.as_str())
                .unwrap_or("本地角色预览"),
        )
        .size(17.0)
        .color(theme::HEADING),
    );
    ui.add_space(10.0);
    app::hint(ui, "选择 PNG 与手臂模型，先预览，再上传到当前正版账号。");
    ui.add_space(18.0);
    let editable = !app.skin.pending && !app.accounts.pending && !app.capes.pending;
    ui.add_enabled_ui(editable, |ui| {
        ui.add(
            egui::TextEdit::singleline(&mut app.skin.path)
                .font(egui::FontId::proportional(13.0))
                .frame(crate::pages::input_frame())
                .margin(Vec2::new(12.0, 10.0))
                .hint_text("PNG 皮肤路径，也可将文件拖入此页")
                .desired_width((width - 24.0).max(0.0)),
        );
        ui.add_space(10.0);
        theme::toolbar(ui, |ui| {
            ui.label("模型");
            for model in [Model::Classic, Model::Slim] {
                if ui
                    .selectable_value(&mut app.skin.model, model, model.label())
                    .changed()
                    && app.skin.local
                {
                    app.skin.image.model = model;
                }
            }
        });
        ui.add_space(10.0);
        theme::toolbar(ui, |ui| {
            if ui.button("预览本地皮肤").clicked() {
                app.skin.preview(ui.ctx());
            }
            if ui
                .add_enabled(
                    app.settings.online_mode && app.accounts.list.current().is_some(),
                    egui::Button::new("上传到当前正版账号"),
                )
                .clicked()
            {
                app.skin.remote(&app.accounts, true, ui.ctx());
            }
            if ui
                .add_enabled(
                    app.settings.online_mode && app.accounts.list.current().is_some(),
                    egui::Button::new("刷新账号皮肤"),
                )
                .clicked()
            {
                app.skin.remote(&app.accounts, false, ui.ctx());
            }
        });
    });
    if editable
        && let Some(path) = ui.ctx().input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")))
        })
    {
        app.skin.path = path.to_string_lossy().into_owned();
        app.skin.preview(ui.ctx());
    }
    ui.add_space(10.0);
    theme::toolbar(ui, |ui| {
        if app.skin.pending {
            ui.spinner();
        }
        ui.label(
            RichText::new(&app.skin.status)
                .color(theme::MUTED)
                .size(12.0),
        );
    });
    app::hint(
        ui,
        "支持 64×64、传统 64×32 PNG 及皮肤外层；上传只修改当前账号的皮肤。",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_click_resets_the_model_and_drag_still_rotates_it() {
        let ctx = egui::Context::default();
        let mut view = SkinView::new();
        view.yaw = 1.5;
        view.pitch = 0.5;
        let rect = Rect::from_min_size(Pos2::new(20.0, 20.0), Vec2::new(200.0, 280.0));
        let pos = rect.center();
        {
            let mut frame = |time, events| {
                ctx.run_ui(
                    egui::RawInput {
                        time: Some(time),
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 400.0))),
                        events,
                        ..Default::default()
                    },
                    |ui| view.paint(ui, rect, "interaction-test"),
                )
                .drop_without_applying_deltas();
            };
            let button = |pressed, pos| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            frame(0.0, vec![]);
            frame(
                0.1,
                vec![
                    egui::Event::PointerMoved(pos),
                    button(true, pos),
                    button(false, pos),
                ],
            );
            frame(0.25, vec![button(true, pos), button(false, pos)]);
            frame(0.4, vec![button(true, pos)]);
            frame(
                0.5,
                vec![egui::Event::PointerMoved(pos + Vec2::new(40.0, 20.0))],
            );
        }
        assert!((view.yaw - 0.2).abs() < 0.001);
        assert!((view.pitch - 0.32).abs() < 0.001);
    }

    #[test]
    fn body_faces_stay_in_atlas_and_rotate_in_depth_order() {
        for arm in [3., 4.] {
            for yaw in [-2., 0., 1.5] {
                let mut faces = Vec::new();
                cube(
                    &mut faces,
                    [0., 0., 0.],
                    [arm, 12., 4.],
                    [48., 48.],
                    0.25,
                    yaw,
                    0.2,
                );
                assert!(!faces.is_empty());
                for face in faces {
                    for uv in face.uvs {
                        assert!((0.0..=64.0).contains(&uv[0]) && (0.0..=64.0).contains(&uv[1]));
                    }
                    assert!(face.depth.is_finite());
                }
            }
        }
    }

    #[test]
    fn skin_controls_and_model_fit_the_account_tab_without_network_actions() {
        let (_root, mut app, ctx) = crate::service::tests::fixture();
        app.accounts.list = Default::default();
        app.account_tab = 1;
        for width in [876.0, 1116.0] {
            let size = egui::vec2(width, 510.0);
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| crate::pages::account(ui, &mut app),
            );
            output.textures_delta.clear();
            for label in ["预览本地皮肤", "上传到当前正版账号", "刷新账号皮肤"]
            {
                let rect = crate::service::tests::text_rect(&output, label);
                assert!(
                    rect.max.x < size.x && rect.max.y < size.y,
                    "{label}: {rect:?}"
                );
            }
            assert!(output.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Mesh(mesh) if mesh.texture_id!=egui::TextureId::default())));
            assert!(!app.skin.pending && !app.accounts.pending);
            assert!(app.service.task == crate::service::Task::Idle);
            output.drop_without_applying_deltas();
        }
    }

    #[test]
    fn late_skin_results_cannot_replace_a_new_account_preview() {
        let mut view = SkinView::new();
        let mut accounts = AccountService::new();
        accounts.list = Default::default();
        view.generation = 2;
        view.pending = true;
        view.status = "current operation".into();
        view.sender
            .send((1, Ok((skins::default_skin(), None, "old account".into()))))
            .unwrap();
        view.sync(&mut accounts, false, &egui::Context::default());
        assert!(view.pending);
        assert_eq!(view.status, "current operation");
        view.sender.send((2, Err("current result".into()))).unwrap();
        view.sync(&mut accounts, false, &egui::Context::default());
        assert!(!view.pending);
        assert_eq!(view.status, "current result");
    }
}
