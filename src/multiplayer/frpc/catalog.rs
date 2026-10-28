use crate::multiplayer::Result;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: String,
    pub name: String,
    pub server_addr: String,
    pub server_port: u16,
    pub public_host: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Proxy {
    pub name: String,
    pub remote_port: u16,
    pub use_encryption: bool,
    pub use_compression: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Node {
    pub info: NodeInfo,
    pub user: String,
    pub login_fail_exit: bool,
    pub heartbeat_interval: u32,
    pub heartbeat_timeout: u32,
    #[serde(default)]
    pub protocol: Option<String>,
    pub proxies: Vec<Proxy>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Route {
    pub info: NodeInfo,
    pub user: String,
    pub login_fail_exit: bool,
    pub heartbeat_interval: u32,
    pub heartbeat_timeout: u32,
    pub protocol: Option<String>,
    pub proxy: Proxy,
}
impl Route {
    pub fn address(&self) -> String {
        format!("{}:{}", self.info.public_host, self.proxy.remote_port)
    }
    pub(crate) fn slot(&self) -> String {
        format!("{}:{}", self.info.id, self.proxy.remote_port)
    }
    pub(crate) fn validate(&self) -> Result<()> {
        Catalog {
            nodes: vec![Node {
                info: self.info.clone(),
                user: self.user.clone(),
                login_fail_exit: self.login_fail_exit,
                heartbeat_interval: self.heartbeat_interval,
                heartbeat_timeout: self.heartbeat_timeout,
                protocol: self.protocol.clone(),
                proxies: vec![self.proxy.clone()],
            }],
        }
        .validate()
    }
    pub(crate) fn config(&self, local_port: u16) -> Result<Vec<u8>> {
        self.validate()?;
        if local_port == 0 {
            return Err("frpc 本地端口无效。".into());
        }
        let mut transport = serde_json::json!({"heartbeatInterval":self.heartbeat_interval,"heartbeatTimeout":self.heartbeat_timeout});
        if let Some(protocol) = &self.protocol {
            transport["protocol"] = protocol.clone().into();
        }
        serde_json::to_vec_pretty(&serde_json::json!({
            "serverAddr":self.info.server_addr,"serverPort":self.info.server_port,
            "user":self.user,"loginFailExit":self.login_fail_exit,"transport":transport,
            "log":{"to":"console","level":"info","disablePrintColor":true},
            "proxies":[{"name":self.proxy.name,"type":"tcp","localIP":"127.0.0.1","localPort":local_port,
                "remotePort":self.proxy.remote_port,"transport":{"useEncryption":self.proxy.use_encryption,
                    "useCompression":self.proxy.use_compression}}]
        })).map_err(|_| "生成 frpc 配置失败。".into())
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub nodes: Vec<Node>,
}
impl Catalog {
    pub fn load(path: &Path) -> Result<Self> {
        use std::io::Read;
        let file = std::fs::File::open(path).map_err(|_| "无法读取 frpc 私有节点配置。")?;
        let mut bytes = Vec::new();
        file.take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "无法读取 frpc 私有节点配置。")?;
        if bytes.len() > 1024 * 1024 {
            return Err("frpc 节点配置过大。".into());
        }
        let catalog: Self =
            serde_json::from_slice(&bytes).map_err(|_| "frpc 节点配置格式无效。")?;
        catalog.validate()?;
        Ok(catalog)
    }
    pub fn validate(&self) -> Result<()> {
        if self.nodes.is_empty() || self.nodes.len() > 32 {
            return Err("frpc 节点数量应为 1–32 个。".into());
        }
        let mut ids = BTreeSet::new();
        for node in &self.nodes {
            if node.info.id.is_empty()
                || node.info.id.len() > 64
                || !node
                    .info
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                || !ids.insert(&node.info.id)
                || !host(&node.info.server_addr)
                || !host(&node.info.public_host)
                || node.info.server_port == 0
                || node.info.name.is_empty()
                || node.info.name.len() > 128
                || node.info.name.chars().any(char::is_control)
                || node.user.is_empty()
                || node.user.len() > 256
                || node.user.chars().any(char::is_control)
                || node.proxies.is_empty()
                || node.proxies.len() > 128
                || !(1..=60).contains(&node.heartbeat_interval)
                || node.heartbeat_timeout < node.heartbeat_interval
                || node.heartbeat_timeout > 600
                || node.protocol.as_ref().is_some_and(|p| {
                    !matches!(p.as_str(), "tcp" | "kcp" | "quic" | "websocket" | "wss")
                })
            {
                return Err("frpc 节点配置包含无效字段或重复节点。".into());
            }
            let (mut names, mut ports) = (BTreeSet::new(), BTreeSet::new());
            for proxy in &node.proxies {
                if proxy.name.is_empty()
                    || proxy.name.len() > 128
                    || !proxy
                        .name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
                    || proxy.remote_port == 0
                    || !names.insert(&proxy.name)
                    || !ports.insert(proxy.remote_port)
                {
                    return Err("frpc 隧道名称或端口无效，或有重复条目。".into());
                }
            }
        }
        Ok(())
    }
    pub(crate) fn route(node: &Node, proxy: &Proxy) -> Route {
        Route {
            info: node.info.clone(),
            user: node.user.clone(),
            login_fail_exit: node.login_fail_exit,
            heartbeat_interval: node.heartbeat_interval,
            heartbeat_timeout: node.heartbeat_timeout,
            protocol: node.protocol.clone(),
            proxy: proxy.clone(),
        }
    }
    /// Provider exports may concatenate several independent TOML documents.
    pub fn import(text: &str) -> Result<Self> {
        if text.len() > 1024 * 1024 {
            return Err("frpc 配置文件过大。".into());
        }
        let mut chunks = Vec::new();
        let mut current = String::new();
        let mut started = false;
        for line in text.lines() {
            if line.trim_start().starts_with("serverAddr")
                && line
                    .split('=')
                    .next()
                    .is_some_and(|k| k.trim() == "serverAddr")
            {
                if started {
                    chunks.push(std::mem::take(&mut current));
                }
                started = true;
            }
            if started {
                current.push_str(line);
                current.push('\n');
            }
        }
        if started {
            chunks.push(current);
        }
        let mut nodes = Vec::new();
        for text in chunks {
            let raw: Raw = toml::from_str(&text)
                .map_err(|_| "frpc TOML 配置无效，请分别保留各节点的完整配置。")?;
            let (id, name, public_host) = match raw.server_addr.as_str() {
                "39.106.189.88" => ("beijing", "北京", "frp-bj.northstarmc.cn"),
                "8.138.226.33" => ("guangzhou", "广州", "frp-gz.northstarmc.cn"),
                "116.62.155.225" => ("hangzhou", "杭州", "frp-hz.northstarmc.cn"),
                "47.100.169.24" => ("shanghai", "上海", "frp-sh.northstarmc.cn"),
                _ => return Err("导入的节点不在已配置的北京、广州、杭州、上海节点中。".into()),
            };
            let mut proxies = Vec::new();
            for proxy in raw.proxies {
                if proxy.kind != "tcp" || proxy.local_ip != "127.0.0.1" {
                    return Err("联机仅支持指向本机的 TCP 隧道。".into());
                }
                proxies.push(Proxy {
                    name: proxy.name,
                    remote_port: proxy.remote_port,
                    use_encryption: proxy.transport.use_encryption,
                    use_compression: proxy.transport.use_compression,
                });
            }
            nodes.push(Node {
                info: NodeInfo {
                    id: id.into(),
                    name: name.into(),
                    public_host: public_host.into(),
                    server_addr: raw.server_addr,
                    server_port: raw.server_port,
                },
                user: raw.user,
                login_fail_exit: raw.login_fail_exit,
                heartbeat_interval: raw.transport.heartbeat_interval,
                heartbeat_timeout: raw.transport.heartbeat_timeout,
                protocol: raw.transport.protocol,
                proxies,
            });
        }
        let catalog = Self { nodes };
        catalog.validate()?;
        Ok(catalog)
    }
}
fn host(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 253
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-".contains(&b))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Raw {
    server_addr: String,
    server_port: u16,
    user: String,
    login_fail_exit: bool,
    transport: RawTransport,
    proxies: Vec<RawProxy>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTransport {
    heartbeat_interval: u32,
    heartbeat_timeout: u32,
    #[serde(default)]
    protocol: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawProxy {
    name: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "localIP")]
    local_ip: String,
    remote_port: u16,
    transport: RawProxyTransport,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawProxyTransport {
    use_encryption: bool,
    use_compression: bool,
}
