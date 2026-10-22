//! OAuth authorization code flow with PKCE and a short-lived, loopback-only listener.
use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream},
};

pub const DEFAULT_REDIRECT_URI: &str = "http://localhost:29111/auth-response";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Clone)]
pub struct BrowserPrompt {
    pub url: String,
    pub redirect_uri: String,
    pub expires: Instant,
}

pub struct BrowserAuthorization {
    prompt: BrowserPrompt,
    listeners: Vec<TcpListener>,
    verifier: String,
    state: String,
}

pub fn validate_redirect_uri(uri: &str) -> Result<()> {
    let url = reqwest::Url::parse(uri).map_err(|_| "登录回调 URI 格式无效。")?;
    if url.scheme() != "http"
        || !matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.port().is_some_and(|port| port >= 1024)
        || url.path() == "/"
    {
        return Err("登录回调必须为带端口和路径的本机 HTTP 地址，例如 http://localhost:29111/auth-response。".into());
    }
    Ok(())
}

fn random_secret() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| "无法生成安全的登录随机数。")?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

impl BrowserAuthorization {
    pub fn new(client_id: &str, redirect_uri: &str) -> Result<Self> {
        Self::with_endpoint(client_id, redirect_uri, &Endpoints::default().microsoft)
    }
    fn with_endpoint(client_id: &str, redirect_uri: &str, endpoint: &str) -> Result<Self> {
        validate_client_id(client_id)?;
        validate_redirect_uri(redirect_uri)?;
        let mut redirect = reqwest::Url::parse(redirect_uri).map_err(|_| "登录回调 URI 无效。")?;
        let preferred = redirect.port().ok_or("登录回调缺少端口。")?;
        let mut ports = vec![preferred];
        // These five native redirect URIs are already registered for NSCL.
        if redirect.host_str() == Some("localhost")
            && redirect.path() == "/auth-response"
            && (29111..=29115).contains(&preferred)
        {
            ports.extend((29111..=29115).filter(|&p| p != preferred));
        }
        let mut bound = None;
        for port in ports {
            if let Ok(listeners) = bind(redirect.host_str().unwrap_or(""), port) {
                bound = Some((port, listeners));
                break;
            }
        }
        let (port, listeners) = bound.ok_or("本地登录回调端口被占用或不可用。请关闭其他登录窗口，或在设置中填写已登记的空闲回调 URI。")?;
        redirect
            .set_port(Some(port))
            .map_err(|_| "登录回调端口无效。")?;
        let verifier = random_secret()?;
        let state = random_secret()?;
        let mut url = reqwest::Url::parse(&format!("{endpoint}/authorize"))
            .map_err(|_| "Microsoft 授权地址无效。")?;
        url.query_pairs_mut().extend_pairs([
            ("client_id", client_id),
            ("response_type", "code"),
            ("response_mode", "query"),
            ("redirect_uri", redirect.as_str()),
            ("scope", SCOPE),
            ("state", &state),
            ("code_challenge", &challenge(&verifier)),
            ("code_challenge_method", "S256"),
            ("prompt", "select_account"),
        ]);
        Ok(Self {
            prompt: BrowserPrompt {
                url: url.into(),
                redirect_uri: redirect.into(),
                expires: Instant::now() + LOGIN_TIMEOUT,
            },
            listeners,
            verifier,
            state,
        })
    }
    pub fn prompt(&self) -> &BrowserPrompt {
        &self.prompt
    }

    fn wait_for_code(&self, cancel: &AtomicBool) -> Result<String> {
        loop {
            check_cancel(cancel)?;
            if Instant::now() >= self.prompt.expires {
                return Err("浏览器登录等待超时，请重新登录。".into());
            }
            for listener in &self.listeners {
                match listener.accept() {
                    Ok((mut stream, peer)) => {
                        if !peer.ip().is_loopback() {
                            continue;
                        }
                        let request = match read_request(&mut stream) {
                            Ok(request) => request,
                            Err(_) => {
                                respond(&mut stream, 400, "无法识别登录回调，请返回启动器重试。");
                                continue;
                            }
                        };
                        match self.callback(&request) {
                            Callback::Ignore(status) => respond(
                                &mut stream,
                                status,
                                "无效的登录回调，请返回启动器继续登录。",
                            ),
                            Callback::Complete(result) => {
                                respond(
                                    &mut stream,
                                    200,
                                    if result.is_ok() {
                                        "已收到 Microsoft 授权，正在验证游戏身份。请返回 NSCL 查看结果。"
                                    } else {
                                        "Microsoft 授权未完成。请返回 NSCL 查看原因。"
                                    },
                                );
                                check_cancel(cancel)?;
                                return result;
                            }
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(_) => return Err("本地登录回调监听失败，请重新登录。".into()),
                }
            }
            std::thread::sleep(Duration::from_millis(40));
        }
    }

    fn callback(&self, request: &str) -> Callback {
        let Ok(redirect) = reqwest::Url::parse(&self.prompt.redirect_uri) else {
            return Callback::Ignore(400);
        };
        let mut lines = request.split("\r\n");
        let Some(first) = lines.next() else {
            return Callback::Ignore(400);
        };
        let parts: Vec<_> = first.split_whitespace().collect();
        if parts.len() != 3
            || parts[0] != "GET"
            || !matches!(parts[2], "HTTP/1.0" | "HTTP/1.1")
            || !parts[1].starts_with('/')
            || parts[1].starts_with("//")
        {
            return Callback::Ignore(400);
        }
        let hosts: Vec<_> = lines
            .filter_map(|line| {
                let (key, value) = line.split_once(':')?;
                key.eq_ignore_ascii_case("host").then_some(value.trim())
            })
            .collect();
        let authority = format!(
            "{}:{}",
            redirect.host_str().unwrap_or(""),
            redirect.port().unwrap_or(0)
        );
        if hosts.len() != 1 || !hosts[0].eq_ignore_ascii_case(&authority) {
            return Callback::Ignore(400);
        }
        let Ok(url) = redirect.join(parts[1]) else {
            return Callback::Ignore(400);
        };
        if url.origin() != redirect.origin() || url.path() != redirect.path() {
            return Callback::Ignore(404);
        }
        if url.fragment().is_some() {
            return Callback::Ignore(400);
        }
        let mut values = std::collections::BTreeMap::new();
        for (key, value) in url.query_pairs() {
            if values
                .insert(key.into_owned(), value.into_owned())
                .is_some()
            {
                return Callback::Ignore(400);
            }
        }
        if !values
            .get("state")
            .is_some_and(|state| constant_time_equal(state.as_bytes(), self.state.as_bytes()))
        {
            return Callback::Ignore(400);
        }
        match (values.get("code"), values.get("error")) {
            (Some(code), None) if !code.is_empty() && code.len() <= 4096 => {
                Callback::Complete(Ok(code.clone()))
            }
            (None, Some(error)) => Callback::Complete(Err(oauth_response_error(
                &json!({"error":error,"error_description":values.get("error_description")}),
            ))),
            _ => Callback::Ignore(400),
        }
    }
}

fn bind(host: &str, port: u16) -> std::io::Result<Vec<TcpListener>> {
    let addresses: Vec<IpAddr> = match host {
        "localhost" => vec![Ipv4Addr::LOCALHOST.into(), Ipv6Addr::LOCALHOST.into()],
        "127.0.0.1" => vec![Ipv4Addr::LOCALHOST.into()],
        "[::1]" => vec![Ipv6Addr::LOCALHOST.into()],
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "loopback required",
            ));
        }
    };
    let mut listeners = Vec::new();
    for ip in addresses {
        match TcpListener::bind(SocketAddr::new(ip, port)) {
            Ok(listener) => {
                listener.set_nonblocking(true)?;
                listeners.push(listener);
            }
            Err(error)
                if ip.is_ipv6()
                    && matches!(
                        error.kind(),
                        std::io::ErrorKind::AddrNotAvailable | std::io::ErrorKind::Unsupported
                    )
                    && !listeners.is_empty() => {}
            Err(error) => return Err(error),
        }
    }
    Ok(listeners)
}
fn constant_time_equal(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |diff, (a, b)| diff | (a ^ b)) == 0
}
fn read_request(stream: &mut TcpStream) -> std::io::Result<String> {
    stream.set_read_timeout(Some(Duration::from_millis(100)))?;
    let deadline = Instant::now() + Duration::from_millis(500);
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 512];
    while bytes.len() < 8192 && Instant::now() < deadline {
        let count = stream.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            return String::from_utf8(bytes[..end].to_vec())
                .map_err(|_| std::io::ErrorKind::InvalidData.into());
        }
    }
    Err(std::io::ErrorKind::InvalidData.into())
}
fn respond(stream: &mut TcpStream, status: u16, message: &str) {
    let page = format!(
        "<!doctype html><html lang=\"zh-CN\"><meta charset=\"utf-8\"><title>NSCL 登录</title><h1>NorthStar Minecraft Launcher</h1><p>{message}</p></html>"
    );
    let _ = stream.set_write_timeout(Some(Duration::from_millis(200)));
    let header = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; frame-ancestors 'none'\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
        if status == 200 { "OK" } else { "Bad Request" },
        page.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(page.as_bytes());
}
enum Callback {
    Ignore(u16),
    Complete(Result<String>),
}

impl AuthClient {
    pub fn login_browser(
        &self,
        client_id: &str,
        redirect_uri: &str,
        cancel: &AtomicBool,
        show: impl FnOnce(&BrowserPrompt),
        stage: impl Fn(&str),
    ) -> Result<Session> {
        check_cancel(cancel)?;
        let authorization = BrowserAuthorization::with_endpoint(
            client_id,
            redirect_uri,
            &self.endpoints.microsoft,
        )?;
        show(authorization.prompt());
        let code = authorization.wait_for_code(cancel)?;
        let redirect_uri = authorization.prompt.redirect_uri.clone();
        let verifier = authorization.verifier.clone();
        drop(authorization);
        stage("正在交换 Microsoft 授权码");
        let token: MicrosoftToken = decode(
            self.client
                .post(format!("{}/token", self.endpoints.microsoft))
                .form(&[
                    ("client_id", client_id),
                    ("grant_type", "authorization_code"),
                    ("code", &code),
                    ("redirect_uri", &redirect_uri),
                    ("code_verifier", &verifier),
                ]),
            "Microsoft 浏览器授权码交换",
        )?;
        check_cancel(cancel)?;
        if token.refresh_token.is_empty() {
            return Err("Microsoft 未授予续期权限，请确认应用允许 offline_access。".into());
        }
        self.exchange(client_id, token, cancel, &stage)
    }
}

#[cfg(test)]
mod tests;
