use super::access::HostIdentity;
use super::{AccessList, CHUNK_SIZE, Member, QUEUE_LIMIT, Result};
use serde::Serialize;
use std::{
    collections::{BTreeMap, VecDeque},
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};
mod frpc;

const MAX_CONNECTIONS: usize = 120;
const MAX_REQUESTS: usize = 192;
const EXPIRES: Duration = Duration::from_secs(60);

struct Room {
    owner: HostIdentity,
    seen: Instant,
    pending: VecDeque<String>,
    direct: Option<frpc::DirectRoom>,
}

#[derive(Default)]
struct Pipe {
    frames: VecDeque<(u64, Vec<u8>)>,
    bytes: usize,
    next: u64,
    acknowledged: Option<u64>,
    finished: bool,
}

struct Connection {
    room: String,
    guest: Member,
    seen: Instant,
    to_host: Pipe,
    to_guest: Pipe,
}

#[derive(Default)]
struct State {
    rooms: BTreeMap<String, Room>,
    connections: BTreeMap<String, Connection>,
}

/// All state is in memory. Credentials and revocations are reloaded for every request.
pub struct RelayServer {
    access_file: PathBuf,
    state: Mutex<State>,
    changed: Condvar,
    frpc_catalog: Option<PathBuf>,
}

impl RelayServer {
    pub fn new(access_file: &Path) -> Result<Self> {
        AccessList::load(access_file)?;
        Ok(Self {
            access_file: access_file.into(),
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
            frpc_catalog: None,
        })
    }

    pub fn with_frpc_catalog(mut self, path: &Path) -> Result<Self> {
        super::frpc::Catalog::load(path)?;
        self.frpc_catalog = Some(path.into());
        Ok(self)
    }

    /// Bind to loopback behind an HTTPS reverse proxy; never publish this HTTP listener directly.
    pub fn serve(self: Arc<Self>, server: Server, stop: Arc<AtomicBool>) -> Result<()> {
        let in_flight = Arc::new(AtomicUsize::new(0));
        while !stop.load(Ordering::Relaxed) {
            let request = server
                .recv_timeout(Duration::from_millis(100))
                .map_err(|_| "联机服务监听失败。")?;
            let Some(request) = request else {
                continue;
            };
            if in_flight.fetch_add(1, Ordering::Relaxed) >= MAX_REQUESTS {
                in_flight.fetch_sub(1, Ordering::Relaxed);
                reply(request, 503, b"relay busy".to_vec());
                continue;
            }
            let (service, active) = (self.clone(), in_flight.clone());
            std::thread::spawn(move || {
                service.handle(request);
                active.fetch_sub(1, Ordering::Relaxed);
            });
        }
        Ok(())
    }

    fn handle(&self, mut request: Request) {
        if request.url().starts_with("/v1/frpc/") {
            self.handle_public_frpc(request);
            return;
        }
        let access = match AccessList::load(&self.access_file) {
            Ok(access) => access,
            Err(_) => {
                reply(request, 503, b"access list unavailable".to_vec());
                return;
            }
        };
        let token = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("Authorization"))
            .and_then(|h| h.value.as_str().strip_prefix("Bearer "));
        let Some(member) = token.and_then(|token| access.authenticate(token)) else {
            reply(request, 401, b"unauthorized".to_vec());
            return;
        };
        if request.body_length().is_some_and(|size| size > CHUNK_SIZE) {
            reply(request, 413, b"payload too large".to_vec());
            return;
        }
        let mut body = Vec::new();
        if request
            .as_reader()
            .take(CHUNK_SIZE as u64 + 1)
            .read_to_end(&mut body)
            .is_err()
            || body.len() > CHUNK_SIZE
        {
            reply(request, 413, b"invalid payload".to_vec());
            return;
        }
        let method = request.method().clone();
        let url = request.url().to_owned();
        let (path, query) = url.split_once('?').unwrap_or((&url, ""));
        let parts: Vec<_> = path.trim_matches('/').split('/').collect();
        let mut state = self.state.lock().unwrap();
        let valid = |member: &Member| {
            access
                .members
                .iter()
                .any(|m| m.identity == member.identity && m.token_sha256 == member.token_sha256)
        };
        state
            .rooms
            .retain(|_, room| room.seen.elapsed() < EXPIRES && access.host_active(&room.owner));
        let rooms: Vec<_> = state.rooms.keys().cloned().collect();
        state.connections.retain(|_, c| {
            c.seen.elapsed() < EXPIRES && rooms.contains(&c.room) && valid(&c.guest)
        });
        match (method, parts.as_slice()) {
            (Method::Get, ["v1", "me"]) => json(request, &member.identity),
            (Method::Post, ["v1", "rooms"]) => {
                if state
                    .rooms
                    .values()
                    .any(|r| r.owner.id == member.identity.id)
                {
                    reply(request, 409, b"one hosted room per account".to_vec());
                    return;
                }
                let id = uuid::Uuid::new_v4().simple().to_string();
                state.rooms.insert(
                    id.clone(),
                    Room {
                        owner: member.into(),
                        seen: Instant::now(),
                        pending: VecDeque::new(),
                        direct: None,
                    },
                );
                json(request, &serde_json::json!({"room":id}));
            }
            (Method::Delete, ["v1", "rooms", room]) => {
                if !state
                    .rooms
                    .get(*room)
                    .is_some_and(|r| r.owner.id == member.identity.id)
                {
                    reply(request, 404, b"room not found".to_vec());
                    return;
                }
                state.rooms.remove(*room);
                state.connections.retain(|_, c| c.room != *room);
                self.changed.notify_all();
                reply(request, 204, vec![]);
            }
            (Method::Get, ["v1", "rooms", room, "events"]) => {
                if !state
                    .rooms
                    .get(*room)
                    .is_some_and(|r| r.owner.id == member.identity.id)
                {
                    reply(request, 404, b"room not found".to_vec());
                    return;
                }
                state.rooms.get_mut(*room).unwrap().seen = Instant::now();
                if state.rooms[*room].pending.is_empty() {
                    state = self
                        .changed
                        .wait_timeout_while(state, Duration::from_secs(1), |s| {
                            s.rooms.get(*room).is_some_and(|r| r.pending.is_empty())
                        })
                        .unwrap()
                        .0;
                }
                let Some(room) = state.rooms.get_mut(*room) else {
                    reply(request, 404, vec![]);
                    return;
                };
                let connections: Vec<_> = room.pending.drain(..).collect();
                json(request, &serde_json::json!({"connections":connections}));
            }
            (Method::Post, ["v1", "rooms", room, "join"]) => {
                if !state.rooms.contains_key(*room) {
                    reply(request, 404, b"room not found".to_vec());
                    return;
                }
                if state.connections.len() >= MAX_CONNECTIONS
                    || state
                        .connections
                        .values()
                        .filter(|c| c.guest.identity.id == member.identity.id)
                        .count()
                        >= 6
                {
                    reply(request, 429, b"connection limit".to_vec());
                    return;
                }
                let id = uuid::Uuid::new_v4().simple().to_string();
                state.connections.insert(
                    id.clone(),
                    Connection {
                        room: (*room).into(),
                        guest: member,
                        seen: Instant::now(),
                        to_host: Pipe::default(),
                        to_guest: Pipe::default(),
                    },
                );
                state
                    .rooms
                    .get_mut(*room)
                    .unwrap()
                    .pending
                    .push_back(id.clone());
                self.changed.notify_all();
                json(request, &serde_json::json!({"connection":id}));
            }
            (Method::Delete, ["v1", "connections", connection]) => {
                if !allowed(&state, connection, &member) {
                    reply(request, 404, vec![]);
                    return;
                }
                if let Some(connection_info) = state.connections.remove(*connection)
                    && let Some(room) = state.rooms.get_mut(&connection_info.room)
                {
                    room.pending.retain(|id| id != *connection);
                }
                self.changed.notify_all();
                reply(request, 204, vec![]);
            }
            (method, ["v1", "connections", connection, side, operation]) => {
                let host = match *side {
                    "host" => true,
                    "guest" => false,
                    _ => {
                        reply(request, 404, vec![]);
                        return;
                    }
                };
                let Some(c) = state.connections.get(*connection) else {
                    reply(request, 410, vec![]);
                    return;
                };
                let permitted = if host {
                    state
                        .rooms
                        .get(&c.room)
                        .is_some_and(|r| r.owner.id == member.identity.id)
                } else {
                    c.guest.identity.id == member.identity.id
                };
                if !permitted {
                    reply(request, 403, vec![]);
                    return;
                }
                state.connections.get_mut(*connection).unwrap().seen = Instant::now();
                if method == Method::Post && *operation == "finish" {
                    let c = state.connections.get_mut(*connection).unwrap();
                    let pipe = if host {
                        &mut c.to_guest
                    } else {
                        &mut c.to_host
                    };
                    pipe.finished = true;
                    self.changed.notify_all();
                    reply(request, 204, vec![]);
                } else if method == Method::Post && *operation == "send" {
                    let sequence = query
                        .strip_prefix("seq=")
                        .and_then(|s| s.parse::<u64>().ok());
                    let c = state.connections.get_mut(*connection).unwrap();
                    let pipe = if host {
                        &mut c.to_guest
                    } else {
                        &mut c.to_host
                    };
                    let Some(sequence) = sequence else {
                        reply(request, 400, vec![]);
                        return;
                    };
                    if sequence < pipe.next {
                        reply(request, 204, vec![]);
                        return;
                    }
                    if sequence != pipe.next || body.is_empty() || pipe.finished {
                        reply(request, 400, vec![]);
                        return;
                    }
                    if pipe.bytes + body.len() > QUEUE_LIMIT {
                        reply(request, 429, vec![]);
                        return;
                    }
                    pipe.bytes += body.len();
                    pipe.frames.push_back((sequence, body));
                    pipe.next += 1;
                    self.changed.notify_all();
                    reply(request, 204, vec![]);
                } else if method == Method::Get && *operation == "receive" {
                    let ack = if query.is_empty() {
                        None
                    } else {
                        match query
                            .strip_prefix("ack=")
                            .and_then(|s| s.parse::<u64>().ok())
                        {
                            Some(ack) => Some(ack),
                            None => {
                                reply(request, 400, vec![]);
                                return;
                            }
                        }
                    };
                    let c = state.connections.get_mut(*connection).unwrap();
                    let pipe = if host {
                        &mut c.to_host
                    } else {
                        &mut c.to_guest
                    };
                    if let Some(ack) = ack {
                        if pipe
                            .frames
                            .front()
                            .is_some_and(|(sequence, _)| *sequence == ack)
                        {
                            let (_, bytes) = pipe.frames.pop_front().unwrap();
                            pipe.bytes -= bytes.len();
                            pipe.acknowledged = Some(ack);
                        } else if pipe.acknowledged != Some(ack) {
                            reply(request, 400, vec![]);
                            return;
                        }
                    }
                    if pipe.frames.is_empty() {
                        state = self
                            .changed
                            .wait_timeout_while(state, Duration::from_secs(1), |s| {
                                s.connections.get(*connection).is_some_and(|c| {
                                    let pipe = if host { &c.to_host } else { &c.to_guest };
                                    pipe.frames.is_empty() && !pipe.finished
                                })
                            })
                            .unwrap()
                            .0;
                    }
                    let Some(c) = state.connections.get(*connection) else {
                        reply(request, 410, vec![]);
                        return;
                    };
                    let pipe = if host { &c.to_host } else { &c.to_guest };
                    let frame = pipe.frames.front().cloned();
                    let finished = pipe.finished;
                    drop(state);
                    match frame {
                        Some((sequence, bytes)) => {
                            let response = Response::from_data(bytes).with_header(
                                Header::from_bytes("X-NSCL-Sequence", sequence.to_string())
                                    .unwrap(),
                            );
                            let _ = request.respond(response);
                        }
                        None => reply(request, if finished { 205 } else { 204 }, vec![]),
                    }
                } else {
                    reply(request, 404, vec![]);
                }
            }
            _ => reply(request, 404, vec![]),
        }
    }
}

fn allowed(state: &State, connection: &str, member: &Member) -> bool {
    state.connections.get(connection).is_some_and(|c| {
        c.guest.identity.id == member.identity.id
            || state
                .rooms
                .get(&c.room)
                .is_some_and(|r| r.owner.id == member.identity.id)
    })
}

fn json(request: Request, value: &impl Serialize) {
    reply(request, 200, serde_json::to_vec(value).unwrap());
}
fn reply(request: Request, status: u16, bytes: Vec<u8>) {
    let _ = request.respond(
        Response::from_data(bytes)
            .with_status_code(StatusCode(status))
            .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap()),
    );
}
