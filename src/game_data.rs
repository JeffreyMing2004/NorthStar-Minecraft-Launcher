//! Read-only world metadata and bounded screenshot previews from an instance.
use crate::{
    instances::{self, Instance},
    minecraft::{Downloader, Result, safe_join},
};
use std::{
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug)]
pub struct World {
    pub folder: String,
    pub name: String,
    pub version: String,
    pub last_played: u64,
    pub game_mode: String,
    pub warning: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Screenshot {
    pub file: String,
    pub modified: u64,
    pub size: u64,
}
#[derive(Default)]
pub struct Catalog {
    pub worlds: Vec<World>,
    pub screenshots: Vec<Screenshot>,
    pub truncated: bool,
}
pub fn checked_path(directory: &Path, relative: &str) -> Result<PathBuf> {
    let path = safe_join(directory, relative)?;
    for p in path.ancestors() {
        instances::reject_link(p)?;
    }
    Ok(path)
}
fn timestamp(t: std::io::Result<SystemTime>) -> u64 {
    t.ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|t| t.as_secs())
        .unwrap_or(0)
}
pub fn list(root: &Path, instance: &Instance, d: &Downloader) -> Result<Catalog> {
    let directory = instance.game_directory(root)?;
    let mut catalog = Catalog::default();
    for folder in ["saves", "screenshots"] {
        let path = checked_path(&directory, folder)?;
        let entries = match fs::read_dir(path) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.to_string()),
        };
        for (count, item) in entries.enumerate() {
            d.check_cancelled()?;
            if count >= 1000 {
                catalog.truncated = true;
                break;
            }
            let item = item.map_err(|e| e.to_string())?;
            let Some(name) = item.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let relative = format!("{folder}/{name}");
            let path = checked_path(&directory, &relative)?;
            let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if folder == "saves" && meta.is_dir() {
                let mut world = World {
                    folder: name.clone(),
                    name,
                    version: "未知版本".into(),
                    last_played: timestamp(meta.modified()),
                    game_mode: "未知模式".into(),
                    warning: None,
                };
                if let Err(e) = world_metadata(
                    &checked_path(&directory, &format!("{relative}/level.dat"))?,
                    &mut world,
                ) {
                    world.warning = Some(e);
                }
                catalog.worlds.push(world);
            } else if folder == "screenshots"
                && meta.is_file()
                && name.to_ascii_lowercase().ends_with(".png")
            {
                catalog.screenshots.push(Screenshot {
                    file: name,
                    modified: timestamp(meta.modified()),
                    size: meta.len(),
                });
            }
        }
    }
    catalog.worlds.sort_by(|a, b| {
        b.last_played
            .cmp(&a.last_played)
            .then(a.folder.cmp(&b.folder))
    });
    catalog
        .screenshots
        .sort_by(|a, b| b.modified.cmp(&a.modified).then(a.file.cmp(&b.file)));
    Ok(catalog)
}
pub fn thumbnail(directory: &Path, screenshot: &Screenshot) -> Result<image::RgbaImage> {
    let path = checked_path(directory, &format!("screenshots/{}", screenshot.file))?;
    let meta = fs::metadata(&path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() > 32 * 1024 * 1024 {
        return Err("截图超过 32 MiB。".into());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let reader = image::ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let (w, h) = reader.into_dimensions().map_err(|_| "截图图片损坏。")?;
    if u64::from(w) * u64::from(h) > 36_000_000 || w == 0 || h == 0 {
        return Err("截图像素数量超限。".into());
    }
    Ok(image::load_from_memory(&bytes)
        .map_err(|_| "无法解码截图。")?
        .thumbnail(320, 180)
        .to_rgba8())
}

// NBT reader skips unrelated tags and arrays; only the world's display fields are kept.
struct Nbt<'a> {
    bytes: &'a [u8],
    at: usize,
    nodes: usize,
}
impl<'a> Nbt<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .filter(|end| *end <= self.bytes.len())
            .ok_or("level.dat 数据不完整。")?;
        let value = &self.bytes[self.at..end];
        self.at = end;
        Ok(value)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn string(&mut self) -> Result<String> {
        let n = u16::from_be_bytes(self.take(2)?.try_into().unwrap()) as usize;
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
    fn payload(&mut self, tag: u8, depth: u32, path: &str, world: &mut World) -> Result<()> {
        self.nodes += 1;
        if depth > 64 || self.nodes > 100_000 {
            return Err("level.dat 层级或标签数量超限。".into());
        }
        match tag {
            1 => {
                self.take(1)?;
            }
            2 => {
                self.take(2)?;
            }
            3 => {
                let n = self.i32()?;
                if path == "Data/GameType" {
                    world.game_mode = match n {
                        0 => "生存",
                        1 => "创造",
                        2 => "冒险",
                        3 => "旁观",
                        _ => "未知模式",
                    }
                    .into();
                }
                if path == "Data/DataVersion" && world.version == "未知版本" {
                    world.version = format!("DataVersion {n}");
                }
            }
            4 => {
                let n = i64::from_be_bytes(self.take(8)?.try_into().unwrap());
                if path == "Data/LastPlayed" && n >= 0 {
                    world.last_played = n as u64 / 1000;
                }
            }
            5 => {
                self.take(4)?;
            }
            6 => {
                self.take(8)?;
            }
            7 | 11 | 12 => {
                let n = usize::try_from(self.i32()?).map_err(|_| "NBT 数组长度无效。")?;
                let width = match tag {
                    11 => 4,
                    12 => 8,
                    _ => 1,
                };
                self.take(n.checked_mul(width).ok_or("NBT 数组过长。")?)?;
            }
            8 => {
                let s = self.string()?;
                if path == "Data/LevelName" {
                    world.name = s.chars().filter(|c| !c.is_control()).take(256).collect();
                }
                if path == "Data/Version/Name" {
                    world.version = s.chars().filter(|c| !c.is_control()).take(128).collect();
                }
            }
            9 => {
                let item = self.u8()?;
                let n = self.i32()?;
                if !(0..=100_000).contains(&n) || item == 0 && n != 0 {
                    return Err("NBT 列表长度无效。".into());
                }
                for _ in 0..n {
                    self.payload(item, depth + 1, "", world)?;
                }
            }
            10 => loop {
                let item = self.u8()?;
                if item == 0 {
                    break;
                }
                let name = self.string()?;
                let next = if depth < 4 {
                    format!("{path}/{name}").trim_start_matches('/').to_owned()
                } else {
                    String::new()
                };
                self.payload(item, depth + 1, &next, world)?;
            },
            _ => return Err("NBT 标签类型无效。".into()),
        }
        Ok(())
    }
}
fn world_metadata(path: &Path, world: &mut World) -> Result<()> {
    let file =
        fs::File::open(path).map_err(|_| "无法读取 level.dat；可打开目录检查或恢复备份。")?;
    if file.metadata().map_err(|e| e.to_string())?.len() > 8 * 1024 * 1024 {
        return Err("level.dat 文件过大。".into());
    }
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(file)
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "level.dat 压缩数据损坏。")?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("level.dat 解压体积超限。".into());
    }
    let mut nbt = Nbt {
        bytes: &bytes,
        at: 0,
        nodes: 0,
    };
    if nbt.u8()? != 10 {
        return Err("level.dat 根标签不是复合类型。".into());
    }
    nbt.string()?;
    nbt.payload(10, 0, "", world)
}
