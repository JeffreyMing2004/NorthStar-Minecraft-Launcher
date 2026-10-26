use crate::{instances::Instance, project_icons, theme};
use egui::{Color32, ColorImage, Context, Rect, Stroke, TextureHandle, TextureOptions, Ui, Vec2};
use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
};

const MAX_WORKERS: usize = 4;
const MAX_ENTRIES: usize = 128;

#[derive(Clone, Copy)]
pub enum Kind {
    Vanilla,
    Fabric,
    Forge,
    NeoForge,
    Pack,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    root: PathBuf,
    id: String,
    version: String,
    archive: Option<String>,
}
struct Entry {
    texture: Option<TextureHandle>,
    loading: bool,
    used: u64,
}
type Loaded = (u64, Key, Option<ColorImage>);

pub struct InstanceIcons {
    entries: HashMap<Key, Entry>,
    sender: Sender<Loaded>,
    receiver: Receiver<Loaded>,
    active: usize,
    generation: u64,
    clock: u64,
}

impl Default for InstanceIcons {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            entries: HashMap::new(),
            sender,
            receiver,
            active: 0,
            generation: 0,
            clock: 0,
        }
    }
}

pub struct Icon {
    texture: Option<TextureHandle>,
    kind: Kind,
}

impl InstanceIcons {
    pub fn clear(&mut self) {
        self.entries.clear();
        self.generation += 1;
    }

    pub fn get(
        &mut self,
        ctx: &Context,
        root: &Path,
        instance: &Instance,
        archive: Option<&str>,
        kind: Kind,
    ) -> Icon {
        while let Ok((generation, key, image)) = self.receiver.try_recv() {
            self.active -= 1;
            if generation == self.generation
                && let Some(entry) = self.entries.get_mut(&key)
            {
                entry.loading = false;
                entry.texture = image.map(|image| {
                    ctx.load_texture(
                        format!("instance-icon:{}:{}", key.id, self.clock),
                        image,
                        TextureOptions::LINEAR,
                    )
                });
            }
        }
        self.clock += 1;
        let key = Key {
            root: root.to_path_buf(),
            id: instance.id.clone(),
            version: instance.version_id.clone(),
            archive: archive.map(str::to_owned),
        };
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.used = self.clock;
            return Icon {
                texture: entry.texture.clone(),
                kind,
            };
        }
        if self.active >= MAX_WORKERS {
            return Icon {
                texture: None,
                kind,
            };
        }
        if self.entries.len() >= MAX_ENTRIES
            && let Some(oldest) = self
                .entries
                .iter()
                .filter(|(_, e)| !e.loading)
                .min_by_key(|(_, e)| e.used)
                .map(|(k, _)| k.clone())
        {
            self.entries.remove(&oldest);
        }
        self.entries.insert(
            key.clone(),
            Entry {
                texture: None,
                loading: true,
                used: self.clock,
            },
        );
        self.active += 1;
        let (sender, ctx, instance, generation) = (
            self.sender.clone(),
            ctx.clone(),
            instance.clone(),
            self.generation,
        );
        let failed_key = key.clone();
        if std::thread::Builder::new()
            .name("instance-icon".into())
            .spawn(move || {
                let image = load(&key.root, &instance, key.archive.as_deref(), kind);
                let _ = sender.send((generation, key, image));
                ctx.request_repaint();
            })
            .is_err()
        {
            self.active -= 1;
            if let Some(entry) = self.entries.get_mut(&failed_key) {
                entry.loading = false;
            }
        }
        Icon {
            texture: None,
            kind,
        }
    }
}

fn color(bytes: &[u8]) -> Option<ColorImage> {
    let image = project_icons::decode(bytes).ok()?;
    Some(ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    ))
}

fn image_file(path: &Path) -> Option<ColorImage> {
    let meta = fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 4 * 1024 * 1024 {
        return None;
    }
    color(&project_icons::read_limited(fs::File::open(path).ok()?).ok()?)
}

fn load(root: &Path, instance: &Instance, archive: Option<&str>, kind: Kind) -> Option<ColorImage> {
    instance.validate().ok()?;
    let own = if let Some(entry) = archive {
        crate::minecraft::validate_id(entry).ok()?;
        root.join("archived-instances").join(entry)
    } else {
        instance.directory(root).ok()?
    };
    let game = if archive.is_some() && instance.isolation == crate::instances::Isolation::Isolated {
        own.clone()
    } else {
        instance.game_directory(root).ok()?
    };
    let version = root.join("versions").join(&instance.version_id);
    let mut candidates = Vec::new();
    for directory in [&own, &game, &version] {
        for name in [
            "icon.png",
            "icon.jpg",
            "icon.webp",
            "pack.png",
            "PCL/Logo.png",
            "logo.png",
        ] {
            let path = directory.join(name);
            if !candidates.contains(&path) {
                candidates.push(path);
            }
        }
    }
    for path in candidates {
        if let Some(image) = image_file(&path) {
            return Some(image);
        }
    }
    if matches!(kind, Kind::Vanilla) {
        let file = fs::File::open(version.join(format!("{}.jar", instance.version_id))).ok()?;
        if file.metadata().ok()?.len() > 512 * 1024 * 1024 {
            return None;
        }
        let mut jar = zip::ZipArchive::new(file).ok()?;
        if jar.len() > 50_000 {
            return None;
        }
        for name in ["pack.png", "icons/icon_32x32.png", "icons/icon_16x16.png"] {
            if let Ok(entry) = jar.by_name(name)
                && entry.size() <= 4 * 1024 * 1024
                && let Ok(bytes) = project_icons::read_limited(entry.take(4 * 1024 * 1024 + 1))
                && let Some(image) = color(&bytes)
            {
                return Some(image);
            }
        }
    }
    None
}

impl Icon {
    pub fn paint(&self, ui: &Ui, rect: Rect) {
        if let Some(texture) = &self.texture {
            let size = texture.size_vec2();
            let scale = (rect.width() / size.x).min(rect.height() / size.y);
            egui::Image::new(texture)
                .paint_at(ui, Rect::from_center_size(rect.center(), size * scale));
            return;
        }
        let p = ui.painter();
        let point = |x: f32, y: f32| rect.min + Vec2::new(x * rect.width(), y * rect.height());
        let polygon = |points: &[(f32, f32)], fill| {
            p.add(egui::Shape::convex_polygon(
                points.iter().map(|&(x, y)| point(x, y)).collect(),
                fill,
                Stroke::NONE,
            ))
        };
        match self.kind {
            Kind::Vanilla | Kind::Pack => {
                let top = if matches!(self.kind, Kind::Vanilla) {
                    Color32::from_rgb(100, 155, 66)
                } else {
                    Color32::from_rgb(186, 149, 90)
                };
                polygon(&[(0.1, 0.28), (0.5, 0.08), (0.9, 0.28), (0.5, 0.49)], top);
                polygon(
                    &[(0.1, 0.28), (0.5, 0.49), (0.5, 0.94), (0.1, 0.73)],
                    Color32::from_rgb(112, 82, 56),
                );
                polygon(
                    &[(0.5, 0.49), (0.9, 0.28), (0.9, 0.73), (0.5, 0.94)],
                    Color32::from_rgb(85, 66, 48),
                );
                if matches!(self.kind, Kind::Pack) {
                    p.line_segment(
                        [point(0.5, 0.1), point(0.5, 0.91)],
                        Stroke::new(rect.width() * 0.1, theme::ACCENT),
                    );
                }
            }
            Kind::Fabric => {
                polygon(
                    &[(0.15, 0.3), (0.65, 0.1), (0.9, 0.65), (0.4, 0.9)],
                    Color32::from_rgb(216, 203, 164),
                );
                for i in 0..4 {
                    let y = 0.35 + i as f32 * 0.12;
                    p.line_segment(
                        [point(0.3, y), point(0.7, y - 0.13)],
                        Stroke::new(1.2, Color32::from_rgb(132, 115, 85)),
                    );
                }
            }
            Kind::Forge => {
                let iron = Color32::from_rgb(147, 156, 164);
                polygon(
                    &[(0.08, 0.18), (0.92, 0.18), (0.75, 0.39), (0.22, 0.39)],
                    iron,
                );
                p.rect_filled(
                    Rect::from_min_max(point(0.41, 0.38), point(0.59, 0.66)),
                    0.0,
                    iron,
                );
                polygon(
                    &[(0.41, 0.65), (0.59, 0.65), (0.84, 0.82), (0.16, 0.82)],
                    iron,
                );
                p.rect_filled(
                    Rect::from_min_max(point(0.16, 0.8), point(0.84, 0.92)),
                    1.0,
                    iron,
                );
                p.rect_filled(
                    Rect::from_min_max(point(0.08, 0.12), point(0.92, 0.22)),
                    1.0,
                    Color32::from_rgb(190, 196, 199),
                );
            }
            Kind::NeoForge => {
                let orange = Color32::from_rgb(210, 119, 48);
                polygon(&[(0.1, 0.05), (0.41, 0.3), (0.09, 0.54)], orange);
                polygon(&[(0.59, 0.3), (0.9, 0.05), (0.91, 0.54)], orange);
                polygon(
                    &[
                        (0.3, 0.3),
                        (0.7, 0.3),
                        (0.92, 0.65),
                        (0.5, 0.97),
                        (0.08, 0.65),
                    ],
                    orange,
                );
                polygon(
                    &[(0.16, 0.54), (0.5, 0.73), (0.5, 0.94)],
                    Color32::from_rgb(239, 220, 187),
                );
                polygon(
                    &[(0.5, 0.73), (0.84, 0.54), (0.5, 0.94)],
                    Color32::from_rgb(239, 220, 187),
                );
                for x in [0.29, 0.65] {
                    p.rect_filled(
                        Rect::from_min_size(point(x, 0.43), Vec2::splat(rect.width() * 0.1)),
                        0.0,
                        Color32::from_rgb(35, 37, 38),
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
