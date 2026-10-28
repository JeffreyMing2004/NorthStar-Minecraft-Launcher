use super::*;
use crate::multiplayer::frpc::{
    Catalog, CreateRoom, Lease, RoomInfo, Route, Status, TicketRequest, tickets::Ticket,
};

pub(super) struct DirectRoom {
    pub key: String,
    pub route: Route,
    pub guests: BTreeMap<String, DirectGuest>,
}
pub(super) struct DirectGuest {
    pub member: Member,
    pub seen: Instant,
}

impl RelayServer {
    pub(super) fn handle_frpc(
        &self,
        request: Request,
        method: Method,
        parts: &[&str],
        body: &[u8],
        member: Member,
        state: &mut State,
    ) {
        match (method, parts) {
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
                            guests: BTreeMap::new(),
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
                let Some(room) = state
                    .rooms
                    .get_mut(*room)
                    .filter(|r| r.owner.identity.id == member.identity.id)
                else {
                    reply(request, 404, vec![]);
                    return;
                };
                let Some(direct) = &room.direct else {
                    reply(request, 404, vec![]);
                    return;
                };
                room.seen = Instant::now();
                json(
                    request,
                    &Status {
                        connections: direct.guests.keys().cloned().collect(),
                    },
                );
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
                let total: usize = state
                    .rooms
                    .values()
                    .filter_map(|r| r.direct.as_ref())
                    .map(|d| d.guests.len())
                    .sum();
                let Some(direct) = state.rooms.get_mut(*room).and_then(|r| r.direct.as_mut())
                else {
                    reply(request, 404, vec![]);
                    return;
                };
                if total >= MAX_CONNECTIONS
                    || direct
                        .guests
                        .values()
                        .filter(|g| g.member.identity.id == member.identity.id)
                        .count()
                        >= 6
                {
                    reply(request, 429, vec![]);
                    return;
                }
                if direct.guests.contains_key(&input.challenge) {
                    reply(request, 409, vec![]);
                    return;
                }
                let Ok(ticket) =
                    Ticket::issue(room, &input.challenge, &member.identity.id, &direct.key)
                else {
                    reply(request, 400, vec![]);
                    return;
                };
                direct.guests.insert(
                    input.challenge,
                    DirectGuest {
                        member,
                        seen: Instant::now(),
                    },
                );
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
                let Some(guest) = state
                    .rooms
                    .get_mut(*room)
                    .and_then(|r| r.direct.as_mut())
                    .and_then(|d| d.guests.get_mut(*challenge))
                    .filter(|g| g.member.identity.id == member.identity.id)
                else {
                    reply(request, 404, vec![]);
                    return;
                };
                guest.seen = Instant::now();
                reply(request, 204, vec![]);
            }
            (Method::Delete, ["v1", "frpc", "rooms", room, "tickets", challenge]) => {
                let Some(room) = state.rooms.get_mut(*room) else {
                    reply(request, 404, vec![]);
                    return;
                };
                let Some(direct) = &mut room.direct else {
                    reply(request, 404, vec![]);
                    return;
                };
                if !direct.guests.get(*challenge).is_some_and(|g| {
                    g.member.identity.id == member.identity.id
                        || room.owner.identity.id == member.identity.id
                }) {
                    reply(request, 404, vec![]);
                    return;
                }
                direct.guests.remove(*challenge);
                reply(request, 204, vec![]);
            }
            _ => reply(request, 404, vec![]),
        }
    }
    fn catalog(&self) -> Option<Catalog> {
        Catalog::load(self.frpc_catalog.as_ref()?).ok()
    }
}
