use nscl::skins::{self, Model, Skin};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub model: Model,
}

pub fn root() -> PathBuf {
    nscl::store::settings_path()
        .with_file_name("cache")
        .join("skin-history")
}
fn account_dir(root: &Path, id: &str) -> Result<PathBuf, String> {
    if !nscl::accounts::valid_profile_id(id) {
        return Err("账号标识无效。".into());
    }
    Ok(root.join(id.to_ascii_lowercase()))
}
pub fn save(root: &Path, id: &str, skin: &Skin) -> Result<(), String> {
    let dir = account_dir(root, id)?;
    fs::create_dir_all(&dir).map_err(|_| "无法创建皮肤库目录。")?;
    let hash = Sha256::digest(skin.pixels.as_raw());
    let name = format!("{:x}-{}.png", hash, skin.model.key());
    let path = dir.join(name);
    let mut encoded = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(skin.pixels.clone())
        .write_to(&mut encoded, image::ImageFormat::Png)
        .map_err(|_| "无法编码皮肤。")?;
    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Ok(()),
        Err(_) => return Err("无法保存皮肤到本地皮肤库。".into()),
    };
    if file.write_all(encoded.get_ref()).is_err() {
        drop(file);
        let _ = fs::remove_file(path);
        return Err("保存皮肤失败。".into());
    }
    Ok(())
}
pub fn list(root: &Path, id: &str) -> Vec<Entry> {
    let Ok(dir) = account_dir(root, id) else {
        return vec![];
    };
    let Ok(files) = fs::read_dir(dir) else {
        return vec![];
    };
    let mut entries: Vec<_> = files
        .flatten()
        .filter_map(|file| {
            if !file.file_type().ok()?.is_file() {
                return None;
            }
            let name = file.file_name().into_string().ok()?;
            let (hash, suffix) = name.split_once('-')?;
            if hash.len() != 64 || !hash.bytes().all(|c| c.is_ascii_hexdigit()) {
                return None;
            }
            let model = match suffix {
                "slim.png" => Model::Slim,
                "classic.png" => Model::Classic,
                _ => return None,
            };
            Some((
                file.metadata().ok()?.modified().ok(),
                Entry {
                    path: file.path(),
                    model,
                },
            ))
        })
        .collect();
    entries.sort_by_key(|a| std::cmp::Reverse(a.0));
    entries.into_iter().take(100).map(|(_, e)| e).collect()
}

#[derive(serde::Deserialize)]
struct Response {
    uuid: String,
    skins: Vec<RemoteSkin>,
}
#[derive(serde::Deserialize)]
struct RemoteSkin {
    texture_key: String,
    slim: bool,
}
fn parse(bytes: &[u8], id: &str) -> Result<Vec<RemoteSkin>, String> {
    let response: Response = serde_json::from_slice(bytes).map_err(|_| "公开皮肤历史格式无效。")?;
    if !response.uuid.replace('-', "").eq_ignore_ascii_case(id) {
        return Err("公开历史返回的账号不匹配。".into());
    }
    Ok(response
        .skins
        .into_iter()
        .filter(|s| {
            !s.texture_key.is_empty()
                && s.texture_key.len() <= 64
                && s.texture_key.bytes().all(|b| b.is_ascii_hexdigit())
        })
        .take(20)
        .collect())
}
pub fn fetch(root: &Path, id: &str) -> Result<usize, String> {
    account_dir(root, id)?;
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("NSCL/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| "无法创建公开历史查询。")?;
    let response = client
        .get(format!(
            "https://mc-api.bisai.dev/v1/profile/{id}/skins?limit=20"
        ))
        .send()
        .map_err(|_| "无法连接公开皮肤历史服务，请稍后重试。")?
        .error_for_status()
        .map_err(|e| {
            format!(
                "公开皮肤历史服务返回错误：{}",
                e.status().map(|s| s.as_u16()).unwrap_or(0)
            )
        })?;
    let mut bytes = Vec::new();
    response
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "读取公开皮肤历史失败。")?;
    if bytes.len() > 1024 * 1024 {
        return Err("公开皮肤历史响应过大。".into());
    }
    let entries = parse(&bytes, id)?;
    let total = entries.len();
    let mut saved = 0;
    for entry in entries {
        let address = format!(
            "https://mc-api.bisai.dev/v1/skin/texture/{}",
            entry.texture_key
        );
        let Ok(response) = client
            .get(address)
            .send()
            .and_then(|r| r.error_for_status())
        else {
            continue;
        };
        let mut data = Vec::new();
        if response
            .take(1024 * 1024 + 1)
            .read_to_end(&mut data)
            .is_err()
        {
            continue;
        }
        let model = if entry.slim {
            Model::Slim
        } else {
            Model::Classic
        };
        if let Ok(skin) = skins::png(&data, model) {
            save(root, id, &skin)?;
            saved += 1;
        }
    }
    if total > 0 && saved == 0 {
        return Err("公开历史中的皮肤下载失败，请稍后重试。".into());
    }
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_is_per_account_deduplicated_and_preserves_model() {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-work");
        fs::create_dir_all(&parent).unwrap();
        let dir = tempfile::tempdir_in(parent).unwrap();
        let a = "a".repeat(32);
        let b = "b".repeat(32);
        let mut skin = skins::default_skin();
        save(dir.path(), &a, &skin).unwrap();
        save(dir.path(), &a, &skin).unwrap();
        skin.model = Model::Slim;
        save(dir.path(), &a, &skin).unwrap();
        assert_eq!(list(dir.path(), &a).len(), 2);
        assert!(list(dir.path(), &b).is_empty());
        assert!(save(dir.path(), "../bad", &skin).is_err());
        assert!(list(dir.path(), &a).iter().any(|e| e.model == Model::Slim));
    }
    #[test]
    fn public_history_rejects_wrong_account_and_untrusted_keys() {
        let id = "a".repeat(32);
        let bytes = format!(
            r#"{{"uuid":"{}","skins":[{{"texture_key":"deadbeef","slim":true}},{{"texture_key":"../../evil","slim":false}}]}}"#,
            id
        );
        assert_eq!(parse(bytes.as_bytes(), &id).unwrap().len(), 1);
        assert!(parse(bytes.as_bytes(), &"b".repeat(32)).is_err());
    }
}
