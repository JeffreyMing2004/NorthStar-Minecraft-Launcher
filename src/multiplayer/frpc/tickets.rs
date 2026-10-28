use crate::multiplayer::Result;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

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
    #[cfg(test)]
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
