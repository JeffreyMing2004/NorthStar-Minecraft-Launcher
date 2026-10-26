/// A readable diagnosis alongside the original transport error for debugging.
pub struct Advice {
    pub title: &'static str,
    pub help: &'static str,
    pub proxy_related: bool,
}

pub fn advice(error: &str) -> Option<Advice> {
    let lower = error.to_ascii_lowercase();
    let proxy = lower.contains("tunnel") || lower.contains("proxy") || lower.contains("代理");
    if lower.contains("os error 10013") || lower.contains("wsaeacces") {
        Some(Advice {
            title: "网络连接被系统拒绝（10013）",
            help: "连接在收到服务器响应前被拒绝。请检查代理是否可用；若不需要代理，可切换直连后重试。若从受限调试环境启动，请从普通桌面重新打开调试版启动器。",
            proxy_related: proxy,
        })
    } else if proxy && (lower.contains("connect") || lower.contains("connection")) {
        Some(Advice {
            title: "无法连接代理服务器",
            help: "请确认代理服务正在运行、地址和端口正确。也可在网络设置中选择直连后重试。更换下载源不会修复不可用的代理。",
            proxy_related: true,
        })
    } else if lower.contains("timed out") || lower.contains("timeout") {
        Some(Advice {
            title: "网络请求超时",
            help: "请重试，或在下载页切换下载源、检查连接方式。已有游戏文件会保留。",
            proxy_related: false,
        })
    } else if lower.contains("error sending request") || lower.contains("tcp connect error") {
        Some(Advice {
            title: "无法连接下载服务器",
            help: "请检查网络与代理设置，或切换下载源后重试。展开技术详情可查看具体原因。",
            proxy_related: false,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denied_proxy_tunnel_is_distinguished_from_a_server_response() {
        let info = advice("error sending request: client error (Connect): tunnel error: tcp connect error: permission denied (os error 10013)").unwrap();
        assert_eq!(info.title, "网络连接被系统拒绝（10013）");
        assert!(info.proxy_related);
        assert!(info.help.contains("调试版"));
        assert!(
            !advice("tcp connect error: permission denied (os error 10013)")
                .unwrap()
                .proxy_related
        );
    }

    #[test]
    fn connection_failures_have_actions_but_non_network_errors_keep_their_message() {
        assert!(
            advice("proxy connect error: connection refused")
                .unwrap()
                .proxy_related
        );
        assert_eq!(advice("operation timed out").unwrap().title, "网络请求超时");
        assert!(advice("error sending request for url").is_some());
        assert!(advice("SHA-1 或文件长度校验失败。").is_none());
        assert!(advice("创建下载目录失败：拒绝访问 (os error 5)").is_none());
    }
}
