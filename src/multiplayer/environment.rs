use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Environment {
    pub minecraft: String,
    pub loader: Option<String>,
    pub loader_version: Option<String>,
    pub pack: Option<crate::content::PackOrigin>,
    pub mods: Vec<Mod>,
    #[serde(default)]
    pub truncated: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mod {
    pub id: String,
    pub sha1: String,
}
impl Environment {
    pub fn validate(&self) -> super::Result<()> {
        crate::minecraft::validate_id(&self.minecraft)?;
        if self.mods.len() > 256
            || self
                .loader
                .as_ref()
                .is_some_and(|s| !matches!(s.as_str(), "forge" | "fabric" | "neoforge" | "quilt"))
            || self
                .loader_version
                .as_ref()
                .is_some_and(|s| s.len() > 128 || s.chars().any(char::is_control))
        {
            return Err("联机环境信息无效。".into());
        }
        if self.pack.as_ref().is_some_and(|p| {
            p.project_id.len() > 128
                || p.version_id.len() > 128
                || p.project_id.chars().any(char::is_control)
                || p.version_id.chars().any(char::is_control)
        }) {
            return Err("整合包来源信息过长或无效。".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for m in &self.mods {
            if m.id.is_empty()
                || m.id.len() > 128
                || !m
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-:".contains(&b))
                || !ids.insert(&m.id)
                || m.sha1.len() != 40
                || !m.sha1.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err("模组环境信息无效。".into());
            }
        }
        Ok(())
    }
    pub fn collect(
        root: &Path,
        instance: &crate::instances::Instance,
        d: &crate::minecraft::Downloader,
    ) -> super::Result<Self> {
        let target = crate::content::Target::from_instance(root, instance)?;
        let directory = instance.game_directory(root)?;
        let registry = crate::content::registry(&directory)?;
        let catalog =
            crate::content::list_local(root, instance, crate::content::LocalKind::Mods, d)?;
        let files: Vec<_> = catalog
            .entries
            .into_iter()
            .filter(|e| e.enabled && !e.missing && !e.directory)
            .collect();
        let mut truncated = files.len() > 256;
        let mut mods = BTreeMap::new();
        for entry in files.into_iter().take(256) {
            d.check_cancelled()?;
            let sha1 = crate::content::manual_file_hash(&directory.join(&entry.relative))?;
            let id = entry.project.unwrap_or_else(|| format!("sha1:{sha1}"));
            if mods.insert(id, sha1).is_some() {
                truncated = true;
            }
        }
        let env = Self {
            minecraft: target.minecraft,
            loader: target.loader.map(|l| l.key().into()),
            loader_version: target.loader_version,
            pack: registry.pack.and_then(|p| p.origin),
            mods: mods
                .into_iter()
                .map(|(id, sha1)| Mod { id, sha1 })
                .collect(),
            truncated,
        };
        env.validate()?;
        Ok(env)
    }
    pub fn differences(&self, remote: &Self) -> Vec<String> {
        let mut result = Vec::new();
        if self.minecraft != remote.minecraft {
            result.push(format!(
                "Minecraft：本机 {}，房主 {}",
                self.minecraft, remote.minecraft
            ));
        }
        if self.loader != remote.loader || self.loader_version != remote.loader_version {
            result.push(format!(
                "加载器：本机 {:?} {:?}，房主 {:?} {:?}",
                self.loader, self.loader_version, remote.loader, remote.loader_version
            ));
        }
        if self.pack != remote.pack {
            result.push("整合包来源或发布版本不同。".into());
        }
        let mut own: Vec<_> = self.mods.iter().collect();
        for m in &remote.mods {
            let matched = own.iter().position(|v| v.id == m.id).or_else(|| {
                own.iter()
                    .position(|v| v.sha1.eq_ignore_ascii_case(&m.sha1))
            });
            if let Some(index) = matched {
                let local = own.remove(index);
                if !local.sha1.eq_ignore_ascii_case(&m.sha1) {
                    result.push(format!("模组文件版本不同：{}", m.id));
                }
            } else {
                result.push(format!("本机缺少房主模组：{}", m.id));
            }
        }
        for m in own {
            result.push(format!("本机额外模组：{}", m.id));
        }
        if self.truncated || remote.truncated {
            result.push("模组摘要不完整（数量超限或来源重复），不能完整比较。".into());
        }
        result
    }
}
#[derive(Clone)]
pub struct LanWorld {
    pub motd: String,
    pub port: u16,
}
pub fn parse_announcement(text: &str) -> Option<LanWorld> {
    let motd = text
        .split_once("[MOTD]")?
        .1
        .split_once("[/MOTD]")?
        .0
        .chars()
        .filter(|c| !c.is_control())
        .take(128)
        .collect();
    let port = text
        .split_once("[AD]")?
        .1
        .split_once("[/AD]")?
        .0
        .parse::<u16>()
        .ok()?;
    if port == 0 {
        return None;
    }
    Some(LanWorld { motd, port })
}
pub fn discover(cancel: &std::sync::atomic::AtomicBool) -> super::Result<Vec<LanWorld>> {
    use std::{
        net::{Ipv4Addr, SocketAddr, TcpStream, UdpSocket},
        sync::atomic::Ordering,
        time::{Duration, Instant},
    };
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 4445))
        .map_err(|e| format!("无法监听局域网公告端口：{e}。也可直接填写本地端口。"))?;
    socket
        .join_multicast_v4(&Ipv4Addr::new(224, 0, 2, 60), &Ipv4Addr::UNSPECIFIED)
        .map_err(|e| e.to_string())?;
    socket
        .set_read_timeout(Some(Duration::from_millis(200)))
        .map_err(|e| e.to_string())?;
    let until = Instant::now() + Duration::from_secs(3);
    let mut worlds = BTreeMap::new();
    let mut buffer = [0; 4096];
    while Instant::now() < until && !cancel.load(Ordering::Relaxed) {
        if let Ok((n, _)) = socket.recv_from(&mut buffer)
            && let Some(world) = parse_announcement(&String::from_utf8_lossy(&buffer[..n]))
            && TcpStream::connect_timeout(
                &SocketAddr::from(([127, 0, 0, 1], world.port)),
                Duration::from_millis(100),
            )
            .is_ok()
        {
            worlds.insert(world.port, world);
        }
    }
    Ok(worlds.into_values().collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lan_announcements_validate_ports() {
        assert_eq!(
            parse_announcement("[MOTD]Local world[/MOTD][AD]51234[/AD]")
                .unwrap()
                .port,
            51234
        );
        assert!(parse_announcement("[MOTD]bad[/MOTD][AD]0[/AD]").is_none());
    }
    #[test]
    fn environment_differences_do_not_claim_complete_compatibility() {
        let a = Environment {
            minecraft: "1.20.1".into(),
            loader: None,
            loader_version: None,
            pack: None,
            mods: vec![],
            truncated: false,
        };
        let mut b = a.clone();
        b.minecraft = "1.21.1".into();
        assert!(!a.differences(&b).is_empty());
        b = a.clone();
        b.truncated = true;
        assert!(!a.differences(&b).is_empty());
    }
    #[test]
    fn identified_and_unidentified_copies_compare_by_file_hash() {
        let local = Environment {
            minecraft: "1.20.1".into(),
            loader: None,
            loader_version: None,
            pack: None,
            mods: vec![Mod {
                id: "project".into(),
                sha1: "a".repeat(40),
            }],
            truncated: false,
        };
        let mut remote = local.clone();
        remote.mods[0].id = format!("sha1:{}", "a".repeat(40));
        assert!(local.differences(&remote).is_empty());
        remote.mods[0].sha1 = "b".repeat(40);
        assert!(!local.differences(&remote).is_empty());
    }
}
