use super::{Result, digest, secret};
use crate::accounts::{Account, AccountKind};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberIdentity {
    pub id: String,
    pub name: String,
    pub kind: AccountKind,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Member {
    pub identity: MemberIdentity,
    pub token_sha256: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostKey {
    pub id: String,
    pub token_sha256: String,
}

#[derive(Clone)]
pub(crate) struct HostIdentity {
    pub id: String,
    pub token_sha256: String,
    pub account: Option<MemberIdentity>,
}
impl From<Member> for HostIdentity {
    fn from(member: Member) -> Self {
        Self {
            id: member.identity.id.clone(),
            token_sha256: member.token_sha256,
            account: Some(member.identity),
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessList {
    #[serde(default)]
    pub members: Vec<Member>,
    #[serde(default)]
    pub host_keys: Vec<HostKey>,
}

impl AccessList {
    pub fn validate(&self) -> Result<()> {
        let (mut ids, mut tokens) = (BTreeSet::new(), BTreeSet::new());
        for member in &self.members {
            let identity = &member.identity;
            if !crate::accounts::valid_profile_id(&identity.id)
                || identity.id != identity.id.to_ascii_lowercase()
                || !ids.insert(&identity.id)
            {
                return Err("白名单包含无效或重复的账号 UUID。".into());
            }
            crate::minecraft::validate_player_name(&identity.name)?;
            if identity.kind == AccountKind::Offline
                && Account::offline(&identity.name)?.id != identity.id
            {
                return Err("离线账号名称与 UUID 不匹配。".into());
            }
            if member.token_sha256.len() != 64
                || !member.token_sha256.bytes().all(|b| b.is_ascii_hexdigit())
                || !tokens.insert(member.token_sha256.to_ascii_lowercase())
            {
                return Err("白名单包含无效或重复的凭证摘要。".into());
            }
        }
        for key in &self.host_keys {
            if !crate::accounts::valid_profile_id(&key.id)
                || key.id != key.id.to_ascii_lowercase()
                || !ids.insert(&key.id)
                || key.token_sha256.len() != 64
                || !key.token_sha256.bytes().all(|b| b.is_ascii_hexdigit())
                || !tokens.insert(key.token_sha256.to_ascii_lowercase())
            {
                return Err("房主凭证包含无效或重复的编号、摘要。".into());
            }
        }
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path).map_err(|_| "无法读取联机白名单。")?;
        if file.metadata().map_err(|_| "无法读取白名单大小。")?.len() > 16 * 1024 * 1024 {
            return Err("联机白名单文件过大。".into());
        }
        let list: Self = serde_json::from_reader(file).map_err(|_| "联机白名单格式错误。")?;
        list.validate()?;
        Ok(list)
    }

    pub(crate) fn authenticate(&self, token: &str) -> Option<Member> {
        if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let hash = digest(token);
        self.members
            .iter()
            .find(|member| constant_eq(&member.token_sha256.to_ascii_lowercase(), &hash))
            .cloned()
    }

    pub(crate) fn authenticate_host(&self, token: &str) -> Option<HostIdentity> {
        if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let hash = digest(token);
        self.host_keys
            .iter()
            .find(|key| constant_eq(&key.token_sha256.to_ascii_lowercase(), &hash))
            .map(|key| HostIdentity {
                id: key.id.clone(),
                token_sha256: key.token_sha256.clone(),
                account: None,
            })
            .or_else(|| self.authenticate(token).map(Into::into))
    }

    pub(crate) fn host_active(&self, host: &HostIdentity) -> bool {
        match &host.account {
            Some(identity) => self
                .members
                .iter()
                .any(|m| &m.identity == identity && m.token_sha256 == host.token_sha256),
            None => self
                .host_keys
                .iter()
                .any(|key| key.id == host.id && key.token_sha256 == host.token_sha256),
        }
    }
}

/// Issue an independent hosting code without collecting a player name or UUID.
pub fn issue_host(list: &mut AccessList) -> Result<(String, String)> {
    list.validate()?;
    let id = uuid::Uuid::new_v4().simple().to_string();
    let token = secret();
    let mut next = list.clone();
    next.host_keys.push(HostKey {
        id: id.clone(),
        token_sha256: digest(&token),
    });
    next.validate()?;
    *list = next;
    Ok((id, token))
}

fn constant_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}

/// The returned credential is shown once to the administrator; only its hash is persisted.
pub fn issue_member(
    list: &mut AccessList,
    kind: AccountKind,
    id: &str,
    name: &str,
) -> Result<String> {
    list.validate()?;
    let id = if kind == AccountKind::Offline {
        Account::offline(name)?.id
    } else {
        id.replace('-', "").to_ascii_lowercase()
    };
    if list.members.iter().any(|m| m.identity.id == id) {
        return Err("该账号已经在联机白名单中。".into());
    }
    let token = secret();
    let mut next = list.clone();
    next.members.push(Member {
        identity: MemberIdentity {
            id,
            name: name.into(),
            kind,
        },
        token_sha256: digest(&token),
    });
    next.validate()?;
    *list = next;
    Ok(token)
}
