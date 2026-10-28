//! Authenticated Minecraft relay. Only the administrator can enroll accounts.
mod access;
mod client;
pub mod frpc;
pub mod server;

pub use access::{AccessList, Member, MemberIdentity, issue_member};
pub use client::{RelayClient, RelaySession};

pub type Result<T> = std::result::Result<T, String>;
pub const CHUNK_SIZE: usize = 64 * 1024;
const QUEUE_LIMIT: usize = 512 * 1024;

pub(crate) fn secret() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

pub(crate) fn digest(value: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

#[cfg(test)]
mod tests;
