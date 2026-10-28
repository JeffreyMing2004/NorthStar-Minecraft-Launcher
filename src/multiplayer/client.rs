use super::{CHUNK_SIZE, MemberIdentity, Result};
use crate::accounts::Account;
use reqwest::{
    Method, StatusCode,
    blocking::{Client, Response},
};
use serde::Deserialize;
use std::{
    io::{Read, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

#[derive(Clone)]
pub struct RelayClient {
    http: Client,
    endpoint: String,
    token: String,
}

#[derive(Deserialize)]
struct RoomReply {
    room: String,
}
#[derive(Deserialize)]
struct ConnectionReply {
    connection: String,
}
#[derive(Deserialize)]
struct Events {
    connections: Vec<String>,
}

impl RelayClient {
    pub fn public(endpoint: &str) -> Result<Self> {
        Self::build(endpoint, String::new())
    }
    pub fn new(endpoint: &str, token: &str) -> Result<Self> {
        if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("联机凭证应为管理员提供的 64 位字符串。".into());
        }
        Self::build(endpoint, token.into())
    }
    fn build(endpoint: &str, token: String) -> Result<Self> {
        let url = reqwest::Url::parse(endpoint.trim())
            .map_err(|_| "请输入完整的联机服务 HTTPS 地址。")?;
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err("联机服务地址不能包含账号、密码、查询或片段。".into());
        }
        let loopback = matches!(url.host_str(), Some("127.0.0.1" | "[::1]" | "localhost"));
        if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
            return Err("公网联机服务必须使用 HTTPS；HTTP 仅用于本机调试。".into());
        }
        let mut builder = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .user_agent(concat!(
                "NorthStar-Minecraft-Launcher/",
                env!("CARGO_PKG_VERSION")
            ));
        if loopback {
            builder = builder.no_proxy();
        }
        Ok(Self {
            http: builder.build().map_err(|_| "创建联机网络客户端失败。")?,
            endpoint: url.as_str().trim_end_matches('/').into(),
            token,
        })
    }

    pub(super) fn request(&self, method: Method, path: &str, body: Vec<u8>) -> Result<Response> {
        let mut request = self
            .http
            .request(method, format!("{}/{}", self.endpoint, path));
        if !self.token.is_empty() {
            request = request.bearer_auth(&self.token);
        }
        request
            .header("Cache-Control", "no-store")
            .body(body)
            .send()
            .map_err(|_| "联机服务连接失败，请检查地址、网络和证书。".into())
    }

    pub(super) fn checked(&self, method: Method, path: &str) -> Result<Response> {
        check(self.request(method, path, vec![])?)
    }

    pub fn authorize(&self, account: &Account) -> Result<MemberIdentity> {
        let member: MemberIdentity = self
            .checked(Method::GET, "v1/me")?
            .json()
            .map_err(|_| "联机服务返回的账号信息无效。")?;
        if member.id != account.id || member.kind != account.kind {
            return Err("该联机凭证不属于当前启动器账号，请切换账号或使用对应凭证。".into());
        }
        Ok(member)
    }

    pub fn host(&self, account: &Account, game_port: u16) -> Result<RelaySession> {
        self.authorize(account)?;
        if game_port == 0 {
            return Err("游戏端口必须为 1–65535。".into());
        }
        // Check the local world before publishing a room; no arbitrary LAN targets are accepted.
        let target = SocketAddr::from(([127, 0, 0, 1], game_port));
        TcpStream::connect_timeout(&target, Duration::from_secs(2))
            .map_err(|_| "无法连接本机游戏端口，请先在游戏内对局域网开放。")?;
        let room: RoomReply = self
            .checked(Method::POST, "v1/rooms")?
            .json()
            .map_err(|_| "联机房间信息无效。")?;
        validate_id(&room.room)?;
        let session = RelaySession::new(self.clone(), room.room, true, None);
        let (client, room, state) = (self.clone(), session.room.clone(), session.state.clone());
        std::thread::spawn(move || {
            let result = client.host_loop(&room, target, state.clone());
            if let Err(error) = result {
                state.fail(error);
            }
            let _ = client.checked(Method::DELETE, &format!("v1/rooms/{room}"));
        });
        Ok(session)
    }

    fn host_loop(&self, room: &str, target: SocketAddr, state: Arc<SessionState>) -> Result<()> {
        while !state.stopped.load(Ordering::Relaxed) {
            let events: Events = self
                .checked(Method::GET, &format!("v1/rooms/{room}/events"))?
                .json()
                .map_err(|_| "联机服务事件格式无效。")?;
            if events.connections.len() > 120 {
                return Err("联机服务返回过多连接。".into());
            }
            for connection in events.connections {
                validate_id(&connection)?;
                if state.stopped.load(Ordering::Relaxed) {
                    break;
                }
                match TcpStream::connect_timeout(&target, Duration::from_secs(2)) {
                    Ok(stream) => self.bridge(connection, true, stream, state.clone()),
                    Err(_) => {
                        let _ =
                            self.checked(Method::DELETE, &format!("v1/connections/{connection}"));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn join(&self, account: &Account, room: &str) -> Result<RelaySession> {
        self.authorize(account)?;
        let room = room.trim();
        validate_id(room)?;
        // Probe authorization/existence now rather than showing a working listener for an invalid room.
        let probe: ConnectionReply = self
            .checked(Method::POST, &format!("v1/rooms/{room}/join"))?
            .json()
            .map_err(|_| "联机服务返回的连接无效。")?;
        validate_id(&probe.connection)?;
        self.checked(
            Method::DELETE,
            &format!("v1/connections/{}", probe.connection),
        )?;
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|_| "无法创建本地联机入口。")?;
        listener
            .set_nonblocking(true)
            .map_err(|_| "无法设置本地联机入口。")?;
        let address = listener
            .local_addr()
            .map_err(|_| "无法读取本地联机端口。")?;
        let session = RelaySession::new(self.clone(), room.into(), false, Some(address));
        let (client, room, state) = (self.clone(), room.to_owned(), session.state.clone());
        std::thread::spawn(move || {
            while !state.stopped.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let (client, room, state) = (client.clone(), room.clone(), state.clone());
                        if state.connections.load(Ordering::Relaxed)
                            + state.pending.load(Ordering::Relaxed)
                            >= 6
                        {
                            drop(stream);
                            continue;
                        }
                        state.pending.fetch_add(1, Ordering::Relaxed);
                        std::thread::spawn(move || {
                            let result = client
                                .checked(Method::POST, &format!("v1/rooms/{room}/join"))
                                .and_then(|r| {
                                    r.json::<ConnectionReply>()
                                        .map_err(|_| "联机服务连接格式无效。".into())
                                });
                            match result {
                                Ok(connection) if validate_id(&connection.connection).is_ok() => {
                                    client.bridge(
                                        connection.connection,
                                        false,
                                        stream,
                                        state.clone(),
                                    )
                                }
                                Ok(_) => state.fail("联机服务连接格式无效。".into()),
                                Err(error) => state.fail(error),
                            }
                            state.pending.fetch_sub(1, Ordering::Relaxed);
                        });
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(25))
                    }
                    Err(_) => {
                        state.fail("本地联机入口已关闭。".into());
                        break;
                    }
                }
            }
        });
        Ok(session)
    }

    fn bridge(
        &self,
        connection: String,
        host: bool,
        mut stream: TcpStream,
        state: Arc<SessionState>,
    ) {
        let client = self.clone();
        state.connections.fetch_add(1, Ordering::Relaxed);
        std::thread::spawn(move || {
            let stopped = Arc::new(AtomicBool::new(false));
            let side = if host { "host" } else { "guest" };
            let path = format!("v1/connections/{connection}/{side}");
            let result = (|| {
                stream
                    .set_nonblocking(false)
                    .map_err(|_| "无法配置联机连接。")?;
                stream
                    .set_read_timeout(Some(Duration::from_millis(250)))
                    .map_err(|_| "无法配置联机连接。")?;
                stream
                    .set_write_timeout(Some(Duration::from_secs(3)))
                    .map_err(|_| "无法配置联机连接。")?;
                stream.set_nodelay(true).map_err(|_| "无法配置联机连接。")?;
                let mut upload = stream.try_clone().map_err(|_| "无法复制联机连接。")?;
                let (sender, send_path, send_stop, send_state) =
                    (client.clone(), path.clone(), stopped.clone(), state.clone());
                let upload_thread = std::thread::spawn(move || {
                    let mut buffer = vec![0; CHUNK_SIZE];
                    let mut sequence = 0u64;
                    let mut eof = false;
                    while !send_stop.load(Ordering::Relaxed)
                        && !send_state.stopped.load(Ordering::Relaxed)
                    {
                        let length = match upload.read(&mut buffer) {
                            Ok(0) => {
                                eof = true;
                                break;
                            }
                            Ok(length) => length,
                            Err(e)
                                if matches!(
                                    e.kind(),
                                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                                ) =>
                            {
                                continue;
                            }
                            Err(_) => break,
                        };
                        loop {
                            if send_stop.load(Ordering::Relaxed)
                                || send_state.stopped.load(Ordering::Relaxed)
                            {
                                break;
                            }
                            match sender.request(
                                Method::POST,
                                &format!("{send_path}/send?seq={sequence}"),
                                buffer[..length].to_vec(),
                            ) {
                                Ok(response)
                                    if response.status() == StatusCode::TOO_MANY_REQUESTS =>
                                {
                                    std::thread::sleep(Duration::from_millis(25))
                                }
                                Ok(response) if response.status().is_success() => {
                                    sequence += 1;
                                    break;
                                }
                                _ => {
                                    send_stop.store(true, Ordering::Relaxed);
                                    break;
                                }
                            }
                        }
                    }
                    if eof
                        && !send_stop.load(Ordering::Relaxed)
                        && !send_state.stopped.load(Ordering::Relaxed)
                    {
                        // Queue EOF after all payload; the receiver acknowledges every frame
                        // before seeing EOF, preserving the final status/login response.
                        if sender
                            .checked(Method::POST, &format!("{send_path}/finish"))
                            .is_err()
                        {
                            send_stop.store(true, Ordering::Relaxed);
                        }
                    } else {
                        send_stop.store(true, Ordering::Relaxed);
                    }
                });
                let mut ack = None;
                let receive_result: Result<()> = (|| {
                    while !stopped.load(Ordering::Relaxed) && !state.stopped.load(Ordering::Relaxed)
                    {
                        let suffix = ack.map(|n| format!("?ack={n}")).unwrap_or_default();
                        let response =
                            client.checked(Method::GET, &format!("{path}/receive{suffix}"))?;
                        if response.status() == StatusCode::NO_CONTENT {
                            continue;
                        }
                        if response.status() == StatusCode::RESET_CONTENT {
                            break;
                        }
                        let sequence = response
                            .headers()
                            .get("X-NSCL-Sequence")
                            .and_then(|h| h.to_str().ok())
                            .and_then(|h| h.parse::<u64>().ok())
                            .ok_or("联机数据序号缺失。")?;
                        if sequence != ack.map_or(0, |n| n + 1) {
                            return Err("联机数据顺序无效。".into());
                        }
                        let mut bytes = Vec::new();
                        response
                            .take(CHUNK_SIZE as u64 + 1)
                            .read_to_end(&mut bytes)
                            .map_err(|_| "读取联机数据失败。")?;
                        if bytes.is_empty() || bytes.len() > CHUNK_SIZE {
                            return Err("联机数据大小无效。".into());
                        }
                        stream.write_all(&bytes).map_err(|_| "游戏连接已断开。")?;
                        ack = Some(sequence);
                    }
                    Ok(())
                })();
                stopped.store(true, Ordering::Relaxed);
                let _ = stream.shutdown(Shutdown::Both);
                let _ = upload_thread.join();
                receive_result
            })();
            let _ = stream.shutdown(Shutdown::Both);
            let _ = client.checked(Method::DELETE, &format!("v1/connections/{connection}"));
            state.connections.fetch_sub(1, Ordering::Relaxed);
            // A normal Minecraft ping/disconnect closes only this stream, not the whole room.
            if let Err(error) = result {
                *state.message.lock().unwrap() = error;
            }
        });
    }
}

fn validate_id(id: &str) -> Result<()> {
    if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("房间码或联机连接标识无效。".into());
    }
    Ok(())
}

fn check(response: Response) -> Result<Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    Err(match response.status().as_u16() {
        401 => "联机凭证无效或已被管理员撤销。",
        403 => "当前账号无权访问该联机连接。",
        404 => "房间不存在、已关闭或不属于当前账号。",
        409 => "此账号已有创建的房间，请先关闭原房间或等待一分钟后重试。",
        410 => "联机连接已关闭。",
        429 => "联机连接数或缓冲区已满，请稍后重试。",
        503 => "联机服务或白名单暂不可用，请联系管理员。",
        _ => "联机服务请求失败。",
    }
    .into())
}

#[derive(Default)]
struct SessionState {
    stopped: AtomicBool,
    connections: AtomicUsize,
    pending: AtomicUsize,
    message: Mutex<String>,
}
impl SessionState {
    fn fail(&self, error: String) {
        *self.message.lock().unwrap() = error;
        self.stopped.store(true, Ordering::Relaxed);
    }
}

pub struct RelaySession {
    client: RelayClient,
    pub room: String,
    pub hosting: bool,
    pub local_address: Option<SocketAddr>,
    state: Arc<SessionState>,
}
impl RelaySession {
    fn new(
        client: RelayClient,
        room: String,
        hosting: bool,
        local_address: Option<SocketAddr>,
    ) -> Self {
        Self {
            client,
            room,
            hosting,
            local_address,
            state: Arc::new(SessionState::default()),
        }
    }
    pub fn running(&self) -> bool {
        !self.state.stopped.load(Ordering::Relaxed)
    }
    pub fn connections(&self) -> usize {
        self.state.connections.load(Ordering::Relaxed)
    }
    pub fn message(&self) -> String {
        self.state.message.lock().unwrap().clone()
    }
    pub fn stop(&self) {
        if !self.state.stopped.swap(true, Ordering::Relaxed) && self.hosting {
            let (client, room) = (self.client.clone(), self.room.clone());
            std::thread::spawn(move || {
                let _ = client.checked(Method::DELETE, &format!("v1/rooms/{room}"));
            });
        }
    }
}
impl Drop for RelaySession {
    fn drop(&mut self) {
        self.stop();
    }
}
