use super::Result;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaunchOptions {
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
    pub custom_resolution: bool,
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
}
impl Default for LaunchOptions {
    fn default() -> Self {
        Self {
            jvm_args: Vec::new(),
            game_args: Vec::new(),
            custom_resolution: false,
            width: 1280,
            height: 720,
            fullscreen: false,
        }
    }
}
impl LaunchOptions {
    pub fn validate(&self) -> Result<()> {
        if !(320..=16384).contains(&self.width) || !(240..=16384).contains(&self.height) {
            return Err("窗口宽度须为 320–16384，高度须为 240–16384。".into());
        }
        for args in [&self.jvm_args, &self.game_args] {
            if args.len() > 128
                || args
                    .iter()
                    .any(|a| a.is_empty() || a.len() > 8192 || a.chars().any(char::is_control))
            {
                return Err(
                    "启动参数每行一个，最多 128 项，每项最多 8192 字节，不能包含控制字符。".into(),
                );
            }
        }
        if self.game_args.iter().any(|a| {
            [
                "--accessToken",
                "--uuid",
                "--username",
                "--gameDir",
                "--clientId",
                "--xuid",
            ]
            .iter()
            .any(|flag| a == flag || a.starts_with(&format!("{flag}=")))
        }) {
            return Err("账号身份与游戏目录由启动器管理，请勿在附加参数中覆盖。".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn options_preserve_argument_boundaries_and_reject_identity_overrides() {
        let mut o = LaunchOptions {
            jvm_args: vec!["-Dname=value with spaces".into()],
            ..Default::default()
        };
        assert!(o.validate().is_ok());
        o.game_args = vec!["--accessToken=private".into()];
        assert!(o.validate().is_err());
        o.game_args.clear();
        o.height = 0;
        assert!(o.validate().is_err());
    }
}
