use super::NodeInfo;
use crate::multiplayer::Result;
use std::{
    net::{TcpStream, ToSocketAddrs},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct NodeLatency {
    pub node: NodeInfo,
    pub latency_ms: u128,
}

/// Randomize probe order and break close-latency ties randomly. This measures TCP latency,
/// not bulk download throughput, against the FRP control port.
pub fn measure_nodes(nodes: &[NodeInfo], cancel: &Arc<AtomicBool>) -> Result<Vec<NodeLatency>> {
    if nodes.len() > 32 {
        return Err("节点数量过多。".into());
    }
    let mut order: Vec<_> = nodes
        .iter()
        .cloned()
        .map(|n| (uuid::Uuid::new_v4(), n))
        .collect();
    order.sort_by_key(|p| p.0);
    let results = std::thread::scope(|scope| {
        let jobs: Vec<_> = order
            .into_iter()
            .map(|(_, node)| {
                scope.spawn(move || {
                    let addresses = (node.server_addr.as_str(), node.server_port)
                        .to_socket_addrs()
                        .ok()?
                        .take(4)
                        .collect::<Vec<_>>();
                    let mut samples = Vec::new();
                    for _ in 0..3 {
                        if cancel.load(Ordering::Relaxed) {
                            return None;
                        }
                        let started = Instant::now();
                        if addresses.iter().any(|addr| {
                            TcpStream::connect_timeout(addr, Duration::from_millis(800)).is_ok()
                        }) {
                            samples.push(started.elapsed().as_millis());
                        }
                    }
                    if samples.is_empty() {
                        None
                    } else {
                        samples.sort_unstable();
                        Some(NodeLatency {
                            node,
                            latency_ms: samples[samples.len() / 2],
                        })
                    }
                })
            })
            .collect();
        jobs.into_iter()
            .filter_map(|j| j.join().ok().flatten())
            .collect::<Vec<_>>()
    });
    if cancel.load(Ordering::Relaxed) {
        return Err("操作已取消。".into());
    }
    let mut results = results;
    results.sort_by_key(|r| r.latency_ms);
    if results.is_empty() {
        return Err("所有 frpc 节点均无法连接，请稍后重试。".into());
    }
    // Within 10 ms, choose a random near-fastest node to distribute rooms.
    let threshold = results[0].latency_ms + 10;
    let best = results
        .iter()
        .take_while(|r| r.latency_ms <= threshold)
        .count();
    let index = (uuid::Uuid::new_v4().as_u128() % (best as u128)) as usize;
    results.swap(0, index);
    Ok(results)
}
