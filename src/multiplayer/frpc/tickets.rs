use crate::multiplayer::Result;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::{
    io::{Read, Write},
    net::TcpStream,
};

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Ticket {
    pub room: String,
    pub challenge: String,
    pub account: String,
    pub expires: u64,
    pub signature: Vec<u8>,
}
impl Ticket {
    fn payload(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(&(&self.room, &self.challenge, &self.account, self.expires))
            .map_err(|_| "联机凭证无效。".into())
    }
    pub fn issue(room: &str, challenge: &str, account: &str, key: &str) -> Result<Self> {
        let mut ticket = Self {
            room: room.into(),
            challenge: challenge.into(),
            account: account.into(),
            expires: crate::accounts::now() + 15,
            signature: vec![],
        };
        let mut mac =
            Hmac::<Sha256>::new_from_slice(key.as_bytes()).map_err(|_| "联机密钥无效。")?;
        mac.update(&ticket.payload()?);
        ticket.signature = mac.finalize().into_bytes().to_vec();
        Ok(ticket)
    }
    pub fn verify(&self, room: &str, challenge: &str, key: &str) -> Result<()> {
        let now = crate::accounts::now();
        if self.room != room
            || self.challenge != challenge
            || self.expires < now
            || self.expires > now + 30
            || !crate::accounts::valid_profile_id(&self.account)
        {
            return Err("联机授权已失效或不属于当前连接。".into());
        }
        let mut mac =
            Hmac::<Sha256>::new_from_slice(key.as_bytes()).map_err(|_| "联机密钥无效。")?;
        mac.update(&self.payload()?);
        mac.verify_slice(&self.signature)
            .map_err(|_| "联机授权签名无效。".into())
    }
}
pub(crate) fn send<T: Serialize>(stream: &mut TcpStream, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec(value).map_err(|_| "联机握手无效。")?;
    if bytes.len() > 4096 {
        return Err("联机握手过大。".into());
    }
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .and_then(|()| stream.write_all(&bytes))
        .map_err(|_| "发送联机握手失败。".into())
}
pub(crate) fn receive<T: serde::de::DeserializeOwned>(stream: &mut TcpStream) -> Result<T> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut length = [0; 4];
    read_until(stream, &mut length, deadline)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > 4096 {
        return Err("联机握手过大。".into());
    }
    let mut bytes = vec![0; length];
    read_until(stream, &mut bytes, deadline)?;
    serde_json::from_slice(&bytes).map_err(|_| "联机握手格式无效。".into())
}
fn read_until(
    stream: &mut TcpStream,
    mut bytes: &mut [u8],
    deadline: std::time::Instant,
) -> Result<()> {
    while !bytes.is_empty() {
        let remaining = deadline
            .checked_duration_since(std::time::Instant::now())
            .ok_or("联机握手超时。")?;
        stream
            .set_read_timeout(Some(remaining))
            .map_err(|_| "无法设置联机握手超时。")?;
        let count = stream.read(bytes).map_err(|_| "读取联机握手失败或超时。")?;
        if count == 0 {
            return Err("联机握手连接已关闭。".into());
        }
        bytes = &mut bytes[count..];
    }
    Ok(())
}
