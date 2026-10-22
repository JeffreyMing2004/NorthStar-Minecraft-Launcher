//! Open login pages without routing authorization URLs through a command shell.

/// Ask the operating system to open an HTTP(S) page in its default browser.
/// Call from a worker thread: the system browser handler may take time to respond.
pub fn open(url: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        open_with_fallback(url, windows_open, default_browser)
    }
    #[cfg(not(windows))]
    {
        validate_url(url)?;
        default_browser(url)
    }
}

fn validate_url(url: &str) -> Result<(), String> {
    if url.chars().any(char::is_control) {
        return Err("登录链接包含无效字符。".into());
    }
    let parsed = reqwest::Url::parse(url).map_err(|_| "登录链接格式无效。")?;
    if !matches!(parsed.scheme(), "https" | "http")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err("仅允许打开 HTTP(S) 网页链接。".into());
    }
    Ok(())
}

fn default_browser(url: &str) -> Result<(), String> {
    webbrowser::open(url).map_err(|error| {
        // Browser helpers may include the URL in their error. Only expose OS codes/kinds.
        match error.raw_os_error() {
            Some(code) => format!("默认浏览器启动失败，系统错误 {code}"),
            None => format!("默认浏览器启动失败，错误类型 {:?}", error.kind()),
        }
    })
}

#[cfg(any(windows, test))]
fn open_with_fallback(
    url: &str,
    primary: impl FnOnce(&str) -> Result<(), String>,
    fallback: impl FnOnce(&str) -> Result<(), String>,
) -> Result<(), String> {
    validate_url(url)?;
    primary(url).or_else(|primary_error| {
        fallback(url).map_err(|fallback_error| {
            format!("{primary_error}；{fallback_error}。请复制登录链接到浏览器中打开。")
        })
    })
}

#[cfg(windows)]
fn windows_open(url: &str) -> Result<(), String> {
    use windows_sys::Win32::{
        System::Com::{
            COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
        },
        UI::{
            Shell::{SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW},
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
    };

    // Shell extensions can require an STA. Balance every successful initialization,
    // including S_FALSE, on this same worker thread.
    let initialized = unsafe {
        CoInitializeEx(
            std::ptr::null(),
            (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
        )
    };
    if initialized < 0 {
        return Err(format!(
            "Windows 浏览器接口初始化失败，代码 {initialized:#010x}"
        ));
    }
    struct ComApartment;
    impl Drop for ComApartment {
        fn drop(&mut self) {
            // SAFETY: created only after a successful CoInitializeEx on this thread.
            unsafe { CoUninitialize() };
        }
    }
    let _apartment = ComApartment;
    let target: Vec<u16> = url.encode_utf16().chain([0]).collect();
    let verb: Vec<u16> = "open".encode_utf16().chain([0]).collect();
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        // The worker has no message loop and exits after this call. Finish the shell
        // handoff before uninitializing COM, and report errors in our own UI.
        fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpVerb: verb.as_ptr(),
        lpFile: target.as_ptr(),
        nShow: SW_SHOWNORMAL,
        ..Default::default()
    };
    // SAFETY: info has the expected size and both NUL-terminated strings stay alive
    // until the synchronous shell handoff finishes. No process handle is requested.
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
        return Err(format!("Windows 无法打开默认浏览器，系统错误 {code}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn falls_back_after_handler_failure_and_preserves_authorization_url() {
        let url = "https://login.live.com/oauth20_authorize.srf?state=a%26b&code_challenge=x-y_z&scope=XboxLive.signin%20offline_access";
        let calls = RefCell::new(Vec::new());
        open_with_fallback(
            url,
            |target| {
                calls.borrow_mut().push(("native", target.to_owned()));
                Err("系统错误 1155".into())
            },
            |target| {
                calls.borrow_mut().push(("direct", target.to_owned()));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            *calls.borrow(),
            [("native", url.to_owned()), ("direct", url.to_owned())]
        );
        open_with_fallback(url, |_| Ok(()), |_| panic!("must not open a second tab")).unwrap();
    }

    #[test]
    fn rejects_non_web_links_before_invoking_os() {
        for url in [
            "file:///C:/Windows/System32/cmd.exe",
            "javascript:alert(1)",
            "ms-settings:defaultapps",
            "https://example.com/\0bad",
            "https://example.com/\nignored",
            "https://user:password@example.com/",
            "not a URL",
        ] {
            assert!(
                open_with_fallback(
                    url,
                    |_| panic!("invalid URL reached OS"),
                    |_| panic!("invalid URL reached fallback")
                )
                .is_err()
            );
        }
    }

    #[test]
    fn both_failures_provide_manual_recovery() {
        let error = open_with_fallback(
            "https://login.live.com/?state=private-test-state",
            |_| Err("系统错误 1155".into()),
            |_| Err("默认浏览器启动失败".into()),
        )
        .unwrap_err();
        assert!(error.contains("1155"));
        assert!(error.contains("复制登录链接"));
        assert!(!error.contains("private-test-state"));
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "opens a real browser; run explicitly for Windows integration verification"]
    fn windows_browser_smoke() {
        // Exercise the native path itself, so a successful fallback cannot hide failure.
        windows_open("https://www.minecraft.net/").unwrap();
    }
}
