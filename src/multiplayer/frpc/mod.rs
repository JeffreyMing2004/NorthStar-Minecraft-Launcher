mod catalog;
mod direct;
pub mod download;
mod probe;
mod process;
pub(crate) mod tickets;

pub use catalog::{Catalog, Node, NodeInfo, Proxy, Route};
pub use direct::{FrpcSession, HostOptions};
pub use probe::{NodeLatency, measure_nodes};
pub const MAX_HOSTS: usize = 28;
pub const SERVICE_NAME: &str = "northstar-multiplayer";
#[derive(Serialize, Deserialize)]
pub struct ServiceInfo {
    pub service: String,
    pub protocol: u32,
    pub max_hosts: usize,
    pub nodes: usize,
    pub tunnels: usize,
}
pub(crate) use process::FrpcProcess;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub(crate) struct CreateRoom {
    pub candidates: Vec<String>,
    #[serde(default)]
    pub avoid: Vec<String>,
}
#[derive(Serialize, Deserialize)]
pub(crate) struct Lease {
    pub room: String,
    pub key: String,
    pub route: Route,
}
#[derive(Serialize, Deserialize)]
pub(crate) struct RoomInfo {
    pub node_name: String,
    pub public_host: String,
    pub remote_port: u16,
}
#[derive(Serialize, Deserialize)]
pub(crate) struct Status {
    pub open: bool,
}
#[derive(Serialize, Deserialize)]
pub(crate) struct TicketRequest {
    pub challenge: String,
}

#[cfg(test)]
mod tests;
