use crate::theme;
use egui::{ColorImage, Context, Rect, TextureHandle, TextureOptions, Ui};
use image::{ImageFormat, ImageReader};
use reqwest::{Url, blocking::Client};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};

const MAX_BYTES: u64 = 4 * 1024 * 1024;
const MAX_WORKERS: usize = 4;
const MAX_TEXTURES: usize = 128;
const THUMBNAIL_SIZE: u32 = 128;
const RETRY_AFTER: Duration = Duration::from_secs(5 * 60);

enum State {
    Loading,
    Ready(TextureHandle),
    Failed(Instant),
}
struct Entry {
    state: State,
    used: u64,
}
type Loaded = (String, Option<ColorImage>);

/// Images have their own bounded workers; browsing never occupies an install task.
pub struct ProjectIcons {
    directory: PathBuf,
    entries: HashMap<String, Entry>,
    sender: Sender<Loaded>,
    receiver: Receiver<Loaded>,
    active: usize,
    clock: u64,
}

pub struct Logo(Option<TextureHandle>);

impl Logo {
    pub fn paint(&self, ui: &Ui, rect: Rect, title: &str) {
        ui.painter().rect_filled(rect, 8.0, theme::CARD_BG);
        if let Some(texture) = &self.0 {
            let size = texture.size_vec2();
            let scale = (rect.width() / size.x).min(rect.height() / size.y);
            egui::Image::new(texture)
                .corner_radius(6)
                .paint_at(ui, Rect::from_center_size(rect.center(), size * scale));
        } else {
            let letter = title.chars().find(|c| !c.is_whitespace()).unwrap_or('N');
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                letter.to_uppercase().to_string(),
                egui::FontId::proportional(rect.height() * 0.4),
                theme::ACCENT,
            );
        }
    }
}

impl ProjectIcons {
    pub fn new(directory: PathBuf) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            directory,
            entries: HashMap::new(),
            sender,
            receiver,
            active: 0,
            clock: 0,
        }
    }

    pub fn get(&mut self, ctx: &Context, url: Option<&str>) -> Logo {
        while let Ok((url, image)) = self.receiver.try_recv() {
            self.active -= 1;
            if let Some(entry) = self.entries.get_mut(&url) {
                entry.state = match image {
                    Some(image) => State::Ready(ctx.load_texture(
                        format!("project-logo:{}", cache_key(&url)),
                        image,
                        TextureOptions::LINEAR,
                    )),
                    None => State::Failed(Instant::now()),
                };
            }
        }
        let Some(url) = url.map(str::trim).filter(|url| allowed_url(url)) else {
            return Logo(None);
        };
        self.clock += 1;
        if let Some(entry) = self.entries.get_mut(url) {
            entry.used = self.clock;
            match &entry.state {
                State::Ready(texture) => return Logo(Some(texture.clone())),
                State::Loading => return Logo(None),
                State::Failed(at) if at.elapsed() < RETRY_AFTER => return Logo(None),
                State::Failed(_) => {}
            }
        }
        if self.active >= MAX_WORKERS {
            return Logo(None);
        }
        if self.entries.len() >= MAX_TEXTURES
            && let Some(oldest) = self
                .entries
                .iter()
                .filter(|(_, entry)| !matches!(entry.state, State::Loading))
                .min_by_key(|(_, entry)| entry.used)
                .map(|(url, _)| url.clone())
        {
            self.entries.remove(&oldest);
        }
        self.entries.insert(
            url.to_owned(),
            Entry {
                state: State::Loading,
                used: self.clock,
            },
        );
        self.active += 1;
        let (directory, url, sender, ctx) = (
            self.directory.clone(),
            url.to_owned(),
            self.sender.clone(),
            ctx.clone(),
        );
        let key = url.clone();
        if std::thread::Builder::new()
            .name("project-logo".into())
            .spawn(move || {
                let image = load(&directory, &url).ok();
                let _ = sender.send((url, image));
                ctx.request_repaint();
            })
            .is_err()
        {
            self.active -= 1;
            self.entries.get_mut(&key).unwrap().state = State::Failed(Instant::now());
        }
        Logo(None)
    }
}

fn cache_key(url: &str) -> String {
    format!("{:x}", Sha256::digest(url.as_bytes()))
}

fn allowed_url(url: &str) -> bool {
    let Ok(url) = Url::parse(url) else {
        return false;
    };
    if !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    #[cfg(test)]
    if url.scheme() == "http" && url.host_str() == Some("127.0.0.1") {
        return true;
    }
    url.scheme() == "https"
        && url.host_str() == Some("cdn.modrinth.com")
        && url.port_or_known_default() == Some(443)
}

pub(crate) fn read_limited(reader: impl Read) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Logo exceeds size limit".into());
    }
    Ok(bytes)
}

pub(crate) fn decode(bytes: &[u8]) -> Result<image::RgbaImage, String> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    if !matches!(
        reader.format(),
        Some(ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP | ImageFormat::Gif)
    ) {
        return Err("Unsupported logo format".into());
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().map_err(|e| e.to_string())?;
    Ok(image.thumbnail(THUMBNAIL_SIZE, THUMBNAIL_SIZE).to_rgba8())
}

fn load(directory: &Path, url: &str) -> Result<ColorImage, String> {
    if !allowed_url(url) {
        return Err("Invalid logo URL".into());
    }
    let path = directory.join(format!("{}.png", cache_key(url)));
    let cached = fs::File::open(&path)
        .map_err(|e| e.to_string())
        .and_then(read_limited)
        .and_then(|bytes| decode(&bytes));
    let image = match cached {
        Ok(image) => image,
        Err(_) => {
            let _ = fs::remove_file(&path);
            let client = Client::builder()
                .user_agent(concat!(
                    "JeffreyMing/NSCL/",
                    env!("CARGO_PKG_VERSION"),
                    " (https://cnb.cool/JeffreyMing/NorthStar-Minecraft-Launcher)"
                ))
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(15))
                .redirect(reqwest::redirect::Policy::custom(|attempt| {
                    if attempt.previous().len() >= 3 || !allowed_url(attempt.url().as_str()) {
                        attempt.error("Invalid logo redirect")
                    } else {
                        attempt.follow()
                    }
                }))
                .build()
                .map_err(|e| e.to_string())?;
            let response = client
                .get(url)
                .send()
                .and_then(reqwest::blocking::Response::error_for_status)
                .map_err(|e| e.to_string())?;
            if response
                .content_length()
                .is_some_and(|size| size > MAX_BYTES)
            {
                return Err("Logo exceeds size limit".into());
            }
            let image = decode(&read_limited(response)?)?;
            // A small normalized PNG keeps subsequent launches fast. Cache errors are harmless.
            let mut png = Cursor::new(Vec::new());
            if image.write_to(&mut png, ImageFormat::Png).is_ok()
                && fs::create_dir_all(directory).is_ok()
            {
                let temporary = directory.join(format!("{}.tmp", uuid::Uuid::new_v4()));
                if fs::write(&temporary, png.into_inner()).is_ok() {
                    let _ = fs::rename(&temporary, &path);
                }
                let _ = fs::remove_file(temporary);
            }
            image
        }
    };
    Ok(ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    ))
}

#[cfg(test)]
mod tests;
