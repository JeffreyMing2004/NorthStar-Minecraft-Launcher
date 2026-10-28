use super::*;
use crate::multiplayer::frpc::{
    Catalog, CreateRoom, Lease, RoomInfo, Route, Status, TicketRequest, tickets::Ticket,
};

pub(super) struct DirectRoom {
    pub key: String,
    pub route: Route,
}

impl RelayServer {
    pub(super) fn handle_public_frpc(&self, mut request: Request) {
        if request.body_length().is_some_and(|n| n > CHUNK_SIZE) {
            reply(request, 413, vec![]);
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
            reply(request, 413, vec![]);
            return;
        }
        let method = request.method().clone();
        let url = request.url().to_owned();
        let parts: Vec<_> = url
            .split('?')
            .next()
            .unwrap_or("")
            .trim_matches('/')
            .split('/')
            .collect();
        let access = match AccessList::load(&self.access_file) {
            Ok(list) => list,
            Err(_) => {
                reply(request, 503, vec![]);
                return;
            }
        };
        let member = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("Authorization"))
            .and_then(|h| h.value.as_str().strip_prefix("Bearer "))
            .and_then(|token| access.authenticate(token));
        let public = matches!(
            (method.clone(), parts.as_slice()),
            (
                Method::Get,
                ["v1", "frpc", "health"] | ["v1", "frpc", "nodes"] | ["v1", "frpc", "rooms", _]
            ) | (
                Method::Post,
                ["v1", "frpc", "rooms", _, "tickets"]
                    | ["v1", "frpc", "rooms", _, "tickets", _, "heartbeat"]
            ) | (Method::Delete, ["v1", "frpc", "rooms", _, "tickets", _])
        );
        if !public && member.is_none() {
            reply(request, 401, vec![]);
            return;
        }
        let mut state = self.state.lock().unwrap();
        state.rooms.retain(|_, room| {
            room.seen.elapsed() < EXPIRES
                && access.members.iter().any(|m| {
                    m.identity == room.owner.identity && m.token_sha256 == room.owner.token_sha256
                })
        });
        self.handle_frpc(request, method, &parts, &body, member, &mut state);
    }
    pub(super) fn handle_frpc(
        &self,
        request: Request,
        method: Method,
        parts: &[&str],
        body: &[u8],
        member: Option<Member>,
        state: &mut State,
    ) {
        match (method, parts) {
            (Method::Get, ["v1", "frpc", "health"]) => {
                let Some(catalog) = self.catalog() else {
                    reply(request, 503, b"frpc catalog unavailable".to_vec());
                    return;
                };
                json(
                    request,
                    &crate::multiplayer::frpc::ServiceInfo {
                        service: crate::multiplayer::frpc::SERVICE_NAME.into(),
                        protocol: 1,
                        max_hosts: crate::multiplayer::frpc::MAX_HOSTS,
                        nodes: catalog.nodes.len(),
                        tunnels: catalog.nodes.iter().map(|n| n.proxies.len()).sum(),
                    },
                );
            }
            (Method::Get, ["v1", "frpc", "nodes"]) => {
                let Some(catalog) = self.catalog() else {
                    reply(request, 503, b"frpc catalog unavailable".to_vec());
                    return;
                };
                json(
                    request,
                    &catalog
                        .nodes
                        .into_iter()
                        .map(|n| n.info)
                        .collect::<Vec<_>>(),
                );
            }
            (Method::Post, ["v1", "frpc", "rooms"]) => {
                let member = member.unwrap();
                if state.rooms.values().filter(|r| r.direct.is_some()).count()
                    >= crate::multiplayer::frpc::MAX_HOSTS
                {
                    reply(request, 409, b"all 28 host slots are occupied".to_vec());
                    return;
                }
                if state
                    .rooms
                    .values()
                    .any(|r| r.owner.identity.id == member.identity.id)
                {
                    reply(request, 409, b"one hosted room per account".to_vec());
                    return;
                }
                let Some(catalog) = self.catalog() else {
                    reply(request, 503, b"frpc catalog unavailable".to_vec());
                    return;
                };
                let Ok(selection) = serde_json::from_slice::<CreateRoom>(body) else {
                    reply(request, 400, vec![]);
                    return;
                };
                let unique: std::collections::BTreeSet<_> = selection.candidates.iter().collect();
                if selection.candidates.is_empty()
                    || selection.candidates.len() > 32
                    || selection.avoid.len() > 128
                    || unique.len() != selection.candidates.len()
                    || selection
                        .candidates
                        .iter()
                        .any(|id| !catalog.nodes.iter().any(|n| &n.info.id == id))
                {
                    reply(request, 400, vec![]);
                    return;
                }
                let used: Vec<_> = state
                    .rooms
                    .values()
                    .filter_map(|r| r.direct.as_ref().map(|d| d.route.slot()))
                    .collect();
                let mut route = None;
                for id in &selection.candidates {
                    let node = catalog.nodes.iter().find(|n| &n.info.id == id).unwrap();
                    let mut proxies: Vec<_> = node
                        .proxies
                        .iter()
                        .map(|p| (uuid::Uuid::new_v4(), p))
                        .collect();
                    proxies.sort_by_key(|p| p.0);
                    for (_, proxy) in proxies {
                        let candidate = Catalog::route(node, proxy);
                        let slot = candidate.slot();
                        if !used.contains(&slot) && !selection.avoid.contains(&slot) {
                            route = Some(candidate);
                            break;
                        }
                    }
                    if route.is_some() {
                        break;
                    }
                }
                let Some(route) = route else {
                    reply(request, 409, b"no available frpc tunnel".to_vec());
                    return;
                };
                let room = uuid::Uuid::new_v4().simple().to_string();
                let key = crate::multiplayer::secret();
                state.rooms.insert(
                    room.clone(),
                    Room {
                        owner: member,
                        seen: Instant::now(),
                        pending: VecDeque::new(),
                        direct: Some(DirectRoom {
                            key: key.clone(),
                            route: route.clone(),
                        }),
                    },
                );
                json(request, &Lease { room, key, route });
            }
            (Method::Get, ["v1", "frpc", "rooms", room]) => {
                let Some(direct) = state.rooms.get(*room).and_then(|r| r.direct.as_ref()) else {
                    reply(request, 404, vec![]);
                    return;
                };
                json(
                    request,
                    &RoomInfo {
                        node_name: direct.route.info.name.clone(),
                        public_host: direct.route.info.public_host.clone(),
                        remote_port: direct.route.proxy.remote_port,
                    },
                );
            }
            (Method::Get, ["v1", "frpc", "rooms", room, "status"]) => {
                let Some(room) = state.rooms.get_mut(*room).filter(|r| {
                    member.as_ref().is_some_and(|m| {
                        r.owner.identity.id == m.identity.id
                            && r.owner.token_sha256 == m.token_sha256
                    })
                }) else {
                    reply(request, 404, vec![]);
                    return;
                };
                let Some(_direct) = &room.direct else {
                    reply(request, 404, vec![]);
                    return;
                };
                room.seen = Instant::now();
                json(request, &Status { open: true });
            }
            (Method::Post, ["v1", "frpc", "rooms", room, "tickets"]) => {
                let Ok(input) = serde_json::from_slice::<TicketRequest>(body) else {
                    reply(request, 400, vec![]);
                    return;
                };
                if input.challenge.len() != 64
                    || !input.challenge.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    reply(request, 400, vec![]);
                    return;
                }
                let Some(direct) = state.rooms.get_mut(*room).and_then(|r| r.direct.as_mut())
                else {
                    reply(request, 404, vec![]);
                    return;
                };
                let guest = uuid::Uuid::new_v4().simple().to_string();
                let Ok(ticket) = Ticket::issue(room, &input.challenge, &guest, &direct.key) else {
                    reply(request, 400, vec![]);
                    return;
                };
                json(request, &ticket);
            }
            (
                Method::Post,
                [
                    "v1",
                    "frpc",
                    "rooms",
                    room,
                    "tickets",
                    challenge,
                    "heartbeat",
                ],
            ) => {
                if !state.rooms.get(*room).is_some_and(|r| r.direct.is_some()) {
                    reply(request, 404, vec![]);
                    return;
                }
                let _ = challenge;
                reply(request, 204, vec![]);
            }
            (Method::Delete, ["v1", "frpc", "rooms", room, "tickets", challenge]) => {
                let Some(room) = state.rooms.get(*room) else {
                    reply(request, 404, vec![]);
                    return;
                };
                let Some(_direct) = &room.direct else {
                    reply(request, 404, vec![]);
                    return;
                };
                let _ = challenge;
                reply(request, 204, vec![]);
            }
            _ => reply(request, 404, vec![]),
        }
    }
    fn catalog(&self) -> Option<Catalog> {
        Catalog::load(self.frpc_catalog.as_ref()?).ok()
    }
}
