//! Minecraft skin profiles and validated PNG uploads. No credentials are sent to texture hosts.
use crate::accounts::{Result, Session};
use image::{ImageFormat, ImageReader, RgbaImage};
use reqwest::blocking::{Client, Response, multipart};
use serde::Deserialize;
use std::{
    io::{Cursor, Read},
    path::Path,
    time::Duration,
};

const MAX_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Model {
    #[default]
    Classic,
    Slim,
}
impl Model {
    pub fn key(self) -> &'static str {
        if self == Self::Slim {
            "slim"
        } else {
            "classic"
        }
    }
    pub fn label(self) -> &'static str {
        if self == Self::Slim {
            "纤细手臂（Alex）"
        } else {
            "标准手臂（Steve）"
        }
    }
}

#[derive(Clone)]
pub struct Skin {
    pub pixels: RgbaImage,
    pub model: Model,
}

pub fn png(bytes: &[u8], model: Model) -> Result<Skin> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err("皮肤文件不能超过 1 MiB。".into());
    }
    if image::guess_format(bytes).ok() != Some(ImageFormat::Png) {
        return Err("请选择 PNG 格式的 Minecraft 皮肤。".into());
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(64);
    limits.max_image_height = Some(64);
    limits.max_alloc = Some(1024 * 1024);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|_| "皮肤 PNG 损坏或尺寸超出 64×64。")?
        .to_rgba8();
    if decoded.width() != 64 || !matches!(decoded.height(), 32 | 64) {
        return Err("皮肤尺寸必须是 64×64 或传统 64×32。".into());
    }
    if decoded.height() == 32 && model == Model::Slim {
        return Err("传统 64×32 皮肤只支持标准手臂。".into());
    }
    let mut pixels = RgbaImage::new(64, 64);
    image::imageops::overlay(&mut pixels, &decoded, 0, 0);
    if decoded.height() == 32 {
        // Legacy textures share mirrored arms/legs. Normalize them to the modern atlas.
        for (source_x, dest_x) in [(0, 16), (40, 32)] {
            for (sx, sy, w, h, dx, dy) in [
                (4, 16, 4, 4, 4, 48),
                (8, 16, 4, 4, 8, 48),
                (0, 20, 4, 12, 8, 52),
                (4, 20, 4, 12, 4, 52),
                (8, 20, 4, 12, 0, 52),
                (12, 20, 4, 12, 12, 52),
            ] {
                let face = image::imageops::crop_imm(&decoded, source_x + sx, sy, w, h).to_image();
                image::imageops::overlay(
                    &mut pixels,
                    &image::imageops::flip_horizontal(&face),
                    (dest_x + dx).into(),
                    dy.into(),
                );
            }
        }
        if (0..16).all(|y| (32..64).all(|x| pixels.get_pixel(x, y)[3] == 255)) {
            for y in 0..16 {
                for x in 32..64 {
                    pixels.get_pixel_mut(x, y)[3] = 0;
                }
            }
        }
    }
    // Minecraft always treats the base head, body and limbs as opaque.
    for (x, y, w, h) in [(0, 0, 32, 16), (0, 16, 64, 16), (16, 48, 32, 16)] {
        for y in y..y + h {
            for x in x..x + w {
                pixels.get_pixel_mut(x, y)[3] = 255;
            }
        }
    }
    Ok(Skin { pixels, model })
}

pub fn read(path: &Path, model: Model) -> Result<(Vec<u8>, Skin)> {
    let file = std::fs::File::open(path).map_err(|_| "无法读取皮肤文件，请检查路径。")?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "读取皮肤文件失败。")?;
    let skin = png(&bytes, model)?;
    Ok((bytes, skin))
}

pub fn default_skin() -> Skin {
    png(include_bytes!("../assets/steve.png"), Model::Classic)
        .expect("embedded Steve texture must be valid")
}

#[derive(Deserialize)]
struct Profile {
    id: String,
    #[serde(default)]
    skins: Vec<ProfileSkin>,
    #[serde(default)]
    capes: Vec<ProfileCape>,
}
#[derive(Deserialize)]
struct ProfileSkin {
    state: String,
    url: String,
    variant: String,
}

#[derive(Deserialize)]
struct ProfileCape {
    id: String,
    state: String,
    url: String,
    #[serde(default)]
    alias: String,
}

#[derive(Clone, Debug)]
pub struct Cape {
    pub id: String,
    pub name: String,
    pub url: String,
    pub active: bool,
}

impl Profile {
    fn capes(self) -> Result<Vec<Cape>> {
        let mut ids = std::collections::BTreeSet::new();
        let mut active = false;
        self.capes
            .into_iter()
            .map(|cape| {
                if cape.id.is_empty()
                    || !matches!(cape.state.as_str(), "ACTIVE" | "INACTIVE")
                    || !ids.insert(cape.id.clone())
                {
                    return Err("披风列表包含无效或重复的标识，请刷新后重试。".into());
                }
                let selected = cape.state == "ACTIVE";
                if selected && active {
                    return Err("披风选中状态无效，请刷新后重试。".into());
                }
                active |= selected;
                Ok(Cape {
                    id: cape.id,
                    name: if cape.alias.trim().is_empty() {
                        "披风".into()
                    } else {
                        cape.alias
                    },
                    url: cape.url,
                    active: selected,
                })
            })
            .collect()
    }
}

pub fn cape_png(bytes: &[u8]) -> Result<RgbaImage> {
    if bytes.len() as u64 > MAX_BYTES || image::guess_format(bytes).ok() != Some(ImageFormat::Png) {
        return Err("披风预览不是有效的 PNG 文件或超过 1 MiB。".into());
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(1024);
    limits.max_image_height = Some(512);
    limits.max_alloc = Some(4 * 1024 * 1024);
    reader.limits(limits);
    let pixels = reader
        .decode()
        .map_err(|_| "无法读取披风预览。")?
        .to_rgba8();
    if pixels.width() < 64
        || !pixels.width().is_multiple_of(64)
        || pixels.height() * 2 != pixels.width()
    {
        return Err("披风预览尺寸无效。".into());
    }
    Ok(pixels)
}

pub struct SkinClient {
    client: Client,
    base: String,
}
impl SkinClient {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(25))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent(concat!("NSCL/", env!("CARGO_PKG_VERSION")))
                .build()
                .map_err(|_| "无法创建皮肤服务连接。")?,
            base: "https://api.minecraftservices.com".into(),
        })
    }
    fn profile(&self, session: &Session) -> Result<Profile> {
        let response = self
            .client
            .get(format!("{}/minecraft/profile", self.base))
            .bearer_auth(&session.access_token)
            .send()
            .map_err(|_| "获取角色档案失败，请检查网络。")?;
        let profile: Profile = checked_for(response, "角色")?
            .json()
            .map_err(|_| "角色档案格式无效。")?;
        if profile.id != session.account.id {
            return Err("角色档案与当前账号不一致，请重新登录。".into());
        }
        Ok(profile)
    }
    pub fn current(&self, session: &Session) -> Result<Skin> {
        let profile = self.profile(session)?;
        let Some(skin) = profile.skins.iter().find(|s| s.state == "ACTIVE") else {
            return Ok(default_skin());
        };
        png(
            &self.texture(&skin.url, "皮肤")?,
            if skin.variant == "SLIM" {
                Model::Slim
            } else {
                Model::Classic
            },
        )
    }
    fn texture(&self, address: &str, kind: &str) -> Result<Vec<u8>> {
        let mut url = reqwest::Url::parse(address).map_err(|_| format!("{kind}地址无效。"))?;
        #[cfg(test)]
        let test_url = url.scheme() == "http" && url.host_str() == Some("127.0.0.1");
        #[cfg(not(test))]
        let test_url = false;
        if !test_url {
            if url.host_str() != Some("textures.minecraft.net")
                || !matches!(url.scheme(), "http" | "https")
                || !url.username().is_empty()
                || url.password().is_some()
                || url.port().is_some()
            {
                return Err(format!("角色档案包含不受信任的{kind}地址。"));
            }
            let _ = url.set_scheme("https");
        }
        let response = self
            .client
            .get(url)
            .send()
            .map_err(|_| format!("下载{kind}失败，请稍后重试。"))?;
        let mut bytes = Vec::new();
        checked_for(response, kind)?
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| format!("读取{kind}失败。"))?;
        Ok(bytes)
    }
    pub fn capes(&self, session: &Session) -> Result<Vec<Cape>> {
        self.profile(session)?.capes()
    }
    pub fn cape_texture(&self, cape: &Cape) -> Result<RgbaImage> {
        cape_png(&self.texture(&cape.url, "披风")?)
    }
    pub fn select_cape(&self, session: &Session, id: Option<&str>) -> Result<Vec<Cape>> {
        let capes = self.capes(session)?;
        if let Some(id) = id
            && !capes.iter().any(|cape| cape.id == id)
        {
            return Err("当前账号不拥有这件披风，请刷新列表。".into());
        }
        if capes.iter().find(|cape| cape.active).map(|c| c.id.as_str()) == id {
            return Ok(capes);
        }
        let address = format!("{}/minecraft/profile/capes/active", self.base);
        let request = if let Some(id) = id {
            self.client
                .put(address)
                .json(&serde_json::json!({ "capeId": id }))
        } else {
            self.client.delete(address)
        };
        checked_for(
            request
                .bearer_auth(&session.access_token)
                .send()
                .map_err(|_| "切换披风失败，请检查网络并刷新列表确认当前状态。")?,
            "披风",
        )?;
        let capes = self.capes(session)?;
        if capes.iter().find(|cape| cape.active).map(|c| c.id.as_str()) != id {
            return Err("披风切换已提交，但服务器状态尚未更新，请刷新列表。".into());
        }
        Ok(capes)
    }
    pub fn upload(&self, session: &Session, bytes: Vec<u8>, model: Model) -> Result<()> {
        png(&bytes, model)?;
        self.profile(session)?;
        let file = multipart::Part::bytes(bytes)
            .file_name("skin.png")
            .mime_str("image/png")
            .map_err(|_| "无法准备皮肤上传。")?;
        let response = self
            .client
            .post(format!("{}/minecraft/profile/skins", self.base))
            .bearer_auth(&session.access_token)
            .multipart(
                multipart::Form::new()
                    .text("variant", model.key())
                    .part("file", file),
            )
            .send()
            .map_err(|_| "上传皮肤失败，请检查网络后重试。")?;
        checked(response)?;
        Ok(())
    }
}

fn checked(response: Response) -> Result<Response> {
    checked_for(response, "皮肤")
}
fn checked_for(response: Response, kind: &str) -> Result<Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    Err(match status.as_u16() {
        400 => format!("{kind}选择无效，请刷新列表后重试。"),
        401 => format!("{kind}服务登录已过期，请重新登录后重试。"),
        403 => format!("{kind}服务拒绝访问，请检查账号授权或联系启动器维护者。"),
        429 => format!("{kind}操作过于频繁，请稍后重试。"),
        _ => format!("{kind}服务请求失败（HTTP {}）。", status.as_u16()),
    })
}

#[cfg(test)]
mod tests;
