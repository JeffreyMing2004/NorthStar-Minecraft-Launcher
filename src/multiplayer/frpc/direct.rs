use super::{
    CreateRoom, FrpcProcess, Lease, NodeInfo, RoomInfo, Status, TicketRequest, measure_nodes,
    tickets::{self, Ticket},
};
use crate::{
    accounts::Account,
    multiplayer::{RelayClient, Result, secret},
};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

pub struct HostOptions {
    pub executable: PathBuf,
    pub runtime_directory: PathBuf,
    pub cancel: Arc<AtomicBool>,
    pub report: Arc<dyn Fn(String) + Send + Sync>,
}
#[derive(Serialize, Deserialize)]
struct Challenge {
    room: String,
    challenge: String,
}
#[derive(Serialize, Deserialize)]
struct Accepted {
    accepted: bool,
}

struct Runtime {
    stopped: Arc<AtomicBool>,
    pending: AtomicUsize,
    connections: Mutex<BTreeMap<String, TcpStream>>,
    message: Mutex<String>,
}
impl Runtime {
    fn new(stopped: Arc<AtomicBool>) -> Arc<Self> {
        Arc::new(Self {
            stopped,
            pending: AtomicUsize::new(0),
            connections: Mutex::new(BTreeMap::new()),
            message: Mutex::new(String::new()),
        })
    }
    fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
        for stream in self.connections.lock().unwrap().values() {
            let _ = stream.shutdown(Shutdown::Both);
        }
    }
    fn fail(&self, message: String) {
        *self.message.lock().unwrap() = message;
        self.stop();
    }
}

pub struct FrpcSession {
    pub room: String,
    pub hosting: bool,
    pub local_address: Option<SocketAddr>,
    pub public_address: String,
    pub node_name: String,
    pub latency_ms: Option<u128>,
    state: Arc<Runtime>,
}
impl FrpcSession {
    pub fn running(&self) -> bool {
        !self.state.stopped.load(Ordering::Relaxed)
    }
    pub fn connections(&self) -> usize {
        self.state.connections.lock().unwrap().len()
    }
    pub fn message(&self) -> String {
        self.state.message.lock().unwrap().clone()
    }
    pub fn stop(&self) {
        self.state.stop();
    }
}
impl Drop for FrpcSession {
    fn drop(&mut self) {
        self.stop();
    }
}

impl RelayClient {
    fn frpc_json<T: serde::de::DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Vec<u8>,
    ) -> Result<T> {
        let response = self.request(method, path, body)?;
        if !response.status().is_success() {
            return Err(frpc_error(response.status().as_u16()));
        }
        let mut bytes = Vec::new();
        response
            .take(64 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "读取 frpc 授权响应失败。")?;
        if bytes.len() > 64 * 1024 {
            return Err("frpc 授权响应过大。".into());
        }
        serde_json::from_slice(&bytes).map_err(|_| "frpc 授权响应格式无效。".into())
    }
    pub fn host_frpc(
        &self,
        _account: &Account,
        game_port: u16,
        options: HostOptions,
    ) -> Result<FrpcSession> {
        self.host_frpc_room(game_port, options)
    }

    pub fn host_frpc_room(&self, game_port: u16, options: HostOptions) -> Result<FrpcSession> {
        if game_port != super::GAME_PORT {
            return Err(
                "仅支持使用本机 25565 端口开房，请将游戏的局域网或服务端端口设置为 25565。".into(),
            );
        }
        self.host_frpc_local(game_port, options)
    }

    pub(super) fn host_frpc_local(
        &self,
        game_port: u16,
        options: HostOptions,
    ) -> Result<FrpcSession> {
        if !options.executable.is_file() {
            return Err("请先选择本地 frpc 程序，下载地址可稍后配置。".into());
        }
        self.check_service()?;
        let target = SocketAddr::from(([127, 0, 0, 1], game_port));
        if game_port == 0 || TcpStream::connect_timeout(&target, Duration::from_secs(2)).is_err() {
            return Err(
                "无法连接本机 25565 端口，请先启动游戏并在 25565 端口对局域网开放。".into(),
            );
        }
        (options.report)("正在随机测速北京、广州、杭州和上海节点……".into());
        let nodes: Vec<NodeInfo> = self.frpc_json(Method::GET, "v1/frpc/nodes", vec![])?;
        let results = measure_nodes(&nodes, &options.cancel)?;
        let mut candidates: Vec<_> = results.iter().map(|r| r.node.id.clone()).collect();
        let mut avoid = Vec::new();
        let listener =
            TcpListener::bind(("127.0.0.1", 0)).map_err(|_| "无法创建本地联机授权入口。")?;
        listener
            .set_nonblocking(true)
            .map_err(|_| "无法配置本地联机入口。")?;
        let port = listener
            .local_addr()
            .map_err(|_| "无法读取本地联机端口。")?
            .port();
        for _ in 0..8 {
            if options.cancel.load(Ordering::Relaxed) {
                return Err("操作已取消。".into());
            }
            if candidates.is_empty() {
                break;
            }
            let selection = serde_json::to_vec(&CreateRoom {
                candidates: candidates.clone(),
                avoid: avoid.clone(),
            })
            .unwrap();
            let lease: Lease = self.frpc_json(Method::POST, "v1/frpc/rooms", selection)?;
            let owner = self.room_client(&lease.owner_token)?;
            if !valid_id(&lease.room)
                || lease.key.len() != 64
                || !lease.key.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err("frpc 房间租约无效。".into());
            }
            if let Err(error) = lease.route.validate() {
                owner.release_room(&lease.room);
                return Err(error);
            }
            (options.report)(format!(
                "正在启动 {} 节点的 frpc 隧道……",
                lease.route.info.name
            ));
            match FrpcProcess::start(
                &options.executable,
                &lease.route,
                port,
                &options.runtime_directory,
                &options.cancel,
            ) {
                Ok(process) => {
                    if options.cancel.load(Ordering::Relaxed) {
                        drop(process);
                        owner.release_room(&lease.room);
                        return Err("操作已取消。".into());
                    }
                    let latency = results
                        .iter()
                        .find(|r| r.node.id == lease.route.info.id)
                        .map(|r| r.latency_ms);
                    return owner.finish_host(
                        lease,
                        process,
                        listener,
                        target,
                        latency,
                        options.cancel,
                    );
                }
                Err(failure) => {
                    owner.release_room(&lease.room);
                    (options.report)(failure.message());
                    if failure == super::process::Failure::Cancelled {
                        return Err(failure.message());
                    }
                    if failure == super::process::Failure::Process {
                        return Err(failure.message());
                    }
                    avoid.push(lease.route.slot());
                    if failure != super::process::Failure::Busy {
                        candidates.retain(|id| id != &lease.route.info.id);
                    }
                }
            }
        }
        Err("可用 frpc 隧道启动失败，请稍后重试或联系管理员。".into())
    }
    fn finish_host(
        &self,
        lease: Lease,
        mut process: FrpcProcess,
        listener: TcpListener,
        target: SocketAddr,
        latency: Option<u128>,
        cancel: Arc<AtomicBool>,
    ) -> Result<FrpcSession> {
        let state = Runtime::new(cancel);
        let session = FrpcSession {
            room: format!("frpc:{}", lease.room),
            hosting: true,
            local_address: None,
            public_address: lease.route.address(),
            node_name: lease.route.info.name.clone(),
            latency_ms: latency,
            state: state.clone(),
        };
        let (client, room, poll_state) = (self.clone(), lease.room.clone(), state.clone());
        std::thread::spawn(move || {
            while !poll_state.stopped.load(Ordering::Relaxed) {
                if !process.running() {
                    poll_state.fail("frpc 进程已退出，房间已关闭。".into());
                    break;
                }
                match client.frpc_json::<Status>(
                    Method::GET,
                    &format!("v1/frpc/rooms/{room}/status"),
                    vec![],
                ) {
                    Ok(status) => {
                        if !status.open {
                            poll_state.fail("房间已关闭。".into());
                            break;
                        }
                    }
                    Err(error) => {
                        poll_state.fail(error);
                        break;
                    }
                }
                pause(&poll_state.stopped, 20);
            }
            poll_state.stop();
            drop(process);
            client.release_room(&room);
        });
        let client = self.clone();
        std::thread::spawn(move || {
            while !state.stopped.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if state.pending.fetch_add(1, Ordering::Relaxed) >= 16 {
                            state.pending.fetch_sub(1, Ordering::Relaxed);
                            drop(stream);
                            continue;
                        }
                        let (client, state, room, key) = (
                            client.clone(),
                            state.clone(),
                            lease.room.clone(),
                            lease.key.clone(),
                        );
                        std::thread::spawn(move || {
                            let result =
                                client.accept_peer(stream, &room, &key, target, state.clone());
                            state.pending.fetch_sub(1, Ordering::Relaxed);
                            // Unauthenticated probes never publish peer data or raw frpc output.
                            if let Err(error) = result {
                                *state.message.lock().unwrap() = error;
                            }
                        });
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(25))
                    }
                    Err(_) => {
                        state.fail("本地 frpc 授权入口已关闭。".into());
                        break;
                    }
                }
            }
        });
        Ok(session)
    }
    fn accept_peer(
        &self,
        mut stream: TcpStream,
        room: &str,
        key: &str,
        target: SocketAddr,
        state: Arc<Runtime>,
    ) -> Result<()> {
        configure(&stream)?;
        let challenge = secret();
        tickets::send(
            &mut stream,
            &Challenge {
                room: room.into(),
                challenge: challenge.clone(),
            },
        )?;
        let ticket: Ticket = tickets::receive(&mut stream)?;
        ticket.verify(room, &challenge, key)?;
        if state.stopped.load(Ordering::Relaxed) {
            return Err("联机已停止。".into());
        }
        let status: Status =
            self.frpc_json(Method::GET, &format!("v1/frpc/rooms/{room}/status"), vec![])?;
        if !status.open {
            return Err("房主的联机权限已撤销或房间已关闭。".into());
        }
        let game = TcpStream::connect_timeout(&target, Duration::from_secs(2))
            .map_err(|_| "房主游戏端口已关闭。")?;
        tickets::send(&mut stream, &Accepted { accepted: true })?;
        self.copy_peer(room.into(), challenge, stream, game, state);
        Ok(())
    }
    pub fn join_frpc(
        &self,
        _account: &Account,
        code: &str,
        cancel: Arc<AtomicBool>,
    ) -> Result<FrpcSession> {
        self.join_frpc_room(code, cancel)
    }
    pub fn join_frpc_room(&self, code: &str, cancel: Arc<AtomicBool>) -> Result<FrpcSession> {
        let room = code
            .trim()
            .strip_prefix("frpc:")
            .ok_or("请填写 frpc 房间码。")?;
        if !valid_id(room) {
            return Err("frpc 房间码无效。".into());
        }
        let info: RoomInfo =
            self.frpc_json(Method::GET, &format!("v1/frpc/rooms/{room}"), vec![])?;
        if info.remote_port == 0
            || info.public_host.len() > 253
            || !info
                .public_host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-".contains(&b))
        {
            return Err("frpc 房间地址无效。".into());
        }
        let probe = self.connect_peer(room, &info)?;
        let _ = probe.0.shutdown(Shutdown::Both);
        self.release_ticket(room, &probe.1);
        if cancel.load(Ordering::Relaxed) {
            return Err("操作已取消。".into());
        }
        let listener =
            TcpListener::bind(("127.0.0.1", 0)).map_err(|_| "无法创建加入房间的本地入口。")?;
        listener
            .set_nonblocking(true)
            .map_err(|_| "无法配置本地联机入口。")?;
        let address = listener
            .local_addr()
            .map_err(|_| "无法读取本地联机端口。")?;
        let state = Runtime::new(cancel);
        let session = FrpcSession {
            room: code.trim().into(),
            hosting: false,
            local_address: Some(address),
            public_address: format!("{}:{}", info.public_host, info.remote_port),
            node_name: info.node_name.clone(),
            latency_ms: None,
            state: state.clone(),
        };
        let (client, room, beat_state) = (self.clone(), room.to_owned(), state.clone());
        let beat_room = room.clone();
        std::thread::spawn(move || {
            while !beat_state.stopped.load(Ordering::Relaxed) {
                let result = client.frpc_json::<RoomInfo>(
                    Method::GET,
                    &format!("v1/frpc/rooms/{beat_room}"),
                    vec![],
                );
                if let Err(error) = result {
                    beat_state.fail(error);
                    break;
                }
                pause(&beat_state.stopped, 20);
            }
        });
        let client = self.clone();
        std::thread::spawn(move || {
            while !state.stopped.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((game, _)) => {
                        state.pending.fetch_add(1, Ordering::Relaxed);
                        let (client, state, room, info) = (
                            client.clone(),
                            state.clone(),
                            room.clone(),
                            RoomInfo {
                                node_name: info.node_name.clone(),
                                public_host: info.public_host.clone(),
                                remote_port: info.remote_port,
                            },
                        );
                        std::thread::spawn(move || {
                            match client.connect_peer(&room, &info) {
                                Ok((stream, challenge)) => {
                                    client.copy_peer(room, challenge, stream, game, state.clone())
                                }
                                Err(error) => state.fail(error),
                            }
                            state.pending.fetch_sub(1, Ordering::Relaxed);
                        });
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(25))
                    }
                    Err(_) => {
                        state.fail("加入房间的本地入口已关闭。".into());
                        break;
                    }
                }
            }
        });
        Ok(session)
    }
    fn connect_peer(&self, room: &str, info: &RoomInfo) -> Result<(TcpStream, String)> {
        let addresses = (info.public_host.as_str(), info.remote_port)
            .to_socket_addrs()
            .map_err(|_| "无法解析房间连接域名。")?;
        let mut stream = addresses
            .take(4)
            .find_map(|a| TcpStream::connect_timeout(&a, Duration::from_secs(3)).ok())
            .ok_or("无法连接 frpc 房间，请检查节点域名和隧道状态。")?;
        configure(&stream)?;
        let challenge: Challenge = tickets::receive(&mut stream)?;
        if challenge.room != room || challenge.challenge.len() != 64 {
            return Err("房间握手不匹配。".into());
        }
        let ticket: Ticket = self.frpc_json(
            Method::POST,
            &format!("v1/frpc/rooms/{room}/tickets"),
            serde_json::to_vec(&TicketRequest {
                challenge: challenge.challenge.clone(),
            })
            .unwrap(),
        )?;
        let result = (|| {
            tickets::send(&mut stream, &ticket)?;
            let accepted: Accepted = tickets::receive(&mut stream)?;
            if !accepted.accepted {
                return Err("房主拒绝了联机授权。".into());
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.release_ticket(room, &challenge.challenge);
            return Err(error);
        }
        Ok((stream, challenge.challenge))
    }
    fn copy_peer(
        &self,
        room: String,
        challenge: String,
        mut tunnel: TcpStream,
        mut game: TcpStream,
        state: Arc<Runtime>,
    ) {
        let client = self.clone();
        let report_state = state.clone();
        let result = (|| -> Result<()> {
            configure(&game)?;
            configure(&tunnel)?;
            game.set_read_timeout(Some(Duration::from_millis(250)))
                .map_err(|_| "无法配置游戏连接。")?;
            tunnel
                .set_read_timeout(Some(Duration::from_millis(250)))
                .map_err(|_| "无法配置联机连接。")?;
            let tracked = tunnel.try_clone().map_err(|_| "无法跟踪联机连接。")?;
            let mut outgoing = tunnel.try_clone().map_err(|_| "无法复制联机连接。")?;
            let mut incoming = game.try_clone().map_err(|_| "无法复制游戏连接。")?;
            if state.stopped.load(Ordering::Relaxed) {
                return Err("联机已停止。".into());
            }
            state
                .connections
                .lock()
                .unwrap()
                .insert(challenge.clone(), tracked);
            let owner = state.clone();
            std::thread::spawn(move || {
                let failed = Arc::new(AtomicBool::new(false));
                let send_failed = failed.clone();
                let send_state = state.clone();
                let upload = std::thread::spawn(move || {
                    pump(&mut game, &mut outgoing, &send_state.stopped, &send_failed)
                });
                pump(&mut tunnel, &mut incoming, &state.stopped, &failed);
                let _ = upload.join();
                let _ = tunnel.shutdown(Shutdown::Both);
                let _ = incoming.shutdown(Shutdown::Both);
                owner.connections.lock().unwrap().remove(&challenge);
                client.release_ticket(&room, &challenge);
            });
            Ok(())
        })();
        if let Err(error) = result {
            *report_state.message.lock().unwrap() = error;
        }
    }
    fn release_ticket(&self, room: &str, challenge: &str) {
        let _ = self.checked(
            Method::DELETE,
            &format!("v1/frpc/rooms/{room}/tickets/{challenge}"),
        );
    }
    fn release_room(&self, room: &str) {
        let _ = self.checked(Method::DELETE, &format!("v1/frpc/rooms/{room}"));
    }
}

fn configure(stream: &TcpStream) -> Result<()> {
    stream
        .set_nonblocking(false)
        .and_then(|()| stream.set_nodelay(true))
        .and_then(|()| stream.set_read_timeout(Some(Duration::from_secs(5))))
        .and_then(|()| stream.set_write_timeout(Some(Duration::from_secs(5))))
        .map_err(|_| "无法配置 frpc 联机连接。".into())
}
fn pump(
    source: &mut TcpStream,
    destination: &mut TcpStream,
    stopped: &AtomicBool,
    failed: &AtomicBool,
) {
    let mut buffer = [0; 65536];
    while !stopped.load(Ordering::Relaxed) && !failed.load(Ordering::Relaxed) {
        match source.read(&mut buffer) {
            Ok(0) => {
                let _ = destination.shutdown(Shutdown::Write);
                return;
            }
            Ok(n) => {
                if destination.write_all(&buffer[..n]).is_err() {
                    break;
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) =>
            {
                continue;
            }
            Err(_) => break,
        }
    }
    failed.store(true, Ordering::Relaxed);
    let _ = source.shutdown(Shutdown::Both);
    let _ = destination.shutdown(Shutdown::Both);
}
fn pause(stopped: &AtomicBool, ticks: usize) {
    for _ in 0..ticks {
        if stopped.load(Ordering::Relaxed) {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
fn valid_id(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn frpc_error(status: u16) -> String {
    match status {
        401 | 403 => "房间管理连接已失效，请重新创建房间。",
        404 => "frpc 房间不存在或已关闭。",
        409 => "28 个开房名额或可用隧道已用完，请等待房间关闭后重试。",
        429 => "联机连接数已达上限，请稍后重试。",
        503 => "管理员尚未配置 frpc 节点，或授权服务暂不可用。",
        _ => "frpc 授权服务请求失败。",
    }
    .into()
}
