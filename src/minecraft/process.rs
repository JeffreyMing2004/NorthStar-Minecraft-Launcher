use super::Result;
use std::{
    process::{Child, ExitStatus},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    time::Duration,
};

#[derive(Clone, Default)]
pub struct GameControl(Arc<State>);
#[derive(Default)]
struct State {
    request: AtomicU8,
    stopped: AtomicBool,
    notice: Mutex<Option<String>>,
}
impl GameControl {
    pub fn request_stop(&self, force: bool) {
        self.0
            .request
            .fetch_max(if force { 2 } else { 1 }, Ordering::SeqCst);
    }
    pub fn stopped(&self) -> bool {
        self.0.stopped.load(Ordering::SeqCst)
    }
    pub fn take_notice(&self) -> Option<String> {
        self.0.notice.lock().ok()?.take()
    }
    fn notice(&self, text: String) {
        if let Ok(mut n) = self.0.notice.lock() {
            *n = Some(text);
        }
    }
}

/// Only the owner of the Child handles requests. No process-name or shell-based kill.
pub(super) fn wait(child: &mut Child, control: &GameControl) -> Result<ExitStatus> {
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return Ok(status);
        }
        match control.0.request.swap(0, Ordering::SeqCst) {
            1 => match close(child) {
                Ok(()) => {
                    control.0.stopped.store(true, Ordering::SeqCst);
                    control.notice("已请求游戏正常关闭；如无响应，可确认后强制结束。".into());
                }
                Err(error) => control.notice(format!(
                    "无法请求正常关闭：{error}。如游戏无响应，可强制结束。"
                )),
            },
            2 => match child.kill() {
                Ok(()) => {
                    control.0.stopped.store(true, Ordering::SeqCst);
                    control.notice("已结束当前游戏进程。".into());
                }
                Err(e) => control.notice(format!("结束游戏失败：{e}")),
            },
            _ => {}
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(windows)]
fn close(child: &Child) -> Result<()> {
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM},
        UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, PostMessageW, WM_CLOSE},
    };
    struct Request {
        pid: u32,
        sent: bool,
    }
    unsafe extern "system" fn visit(hwnd: HWND, data: LPARAM) -> i32 {
        // SAFETY: EnumWindows invokes this synchronously with a live Request pointer.
        let request = unsafe { &mut *(data as *mut Request) };
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut pid);
        }
        if pid == request.pid && unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) } != 0 {
            request.sent = true;
        }
        1
    }
    let mut request = Request {
        pid: child.id(),
        sent: false,
    };
    // The owned Child retains its process handle, so this PID cannot be recycled here.
    unsafe {
        EnumWindows(Some(visit), &mut request as *mut Request as LPARAM);
    }
    if request.sent {
        Ok(())
    } else {
        Err("没有可关闭的游戏窗口".into())
    }
}
#[cfg(unix)]
fn close(child: &Child) -> Result<()> {
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    // The parent has not reaped this child, so its PID cannot have been recycled.
    if unsafe { kill(child.id() as i32, 15) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().to_string())
    }
}
#[cfg(not(any(windows, unix)))]
fn close(_: &Child) -> Result<()> {
    Err("此系统不支持正常关闭请求".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    #[ignore = "helper window runs only as the graceful-stop test's owned child"]
    fn graceful_window_child() {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        let Some(path) = std::env::var_os("NSCL_TEST_CLOSE_READY") else {
            return;
        };
        let class: Vec<_> = "STATIC\0".encode_utf16().collect();
        // A hidden test window exercises the same message path as a game window.
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                class.as_ptr(),
                WS_OVERLAPPEDWINDOW,
                0,
                0,
                100,
                100,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        assert!(!hwnd.is_null());
        std::fs::write(path, "ready").unwrap();
        let mut message = MSG::default();
        while unsafe { IsWindow(hwnd) } != 0
            && unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0
        {
            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
    #[cfg(windows)]
    #[test]
    fn graceful_stop_requests_close_of_the_owned_window() {
        let parent = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-work");
        std::fs::create_dir_all(&parent).unwrap();
        let root = tempfile::tempdir_in(parent).unwrap();
        let ready = root.path().join("ready");
        let mut child = crate::minecraft::hidden_command(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "minecraft::process::tests::graceful_window_child",
                "--ignored",
            ])
            .env("NSCL_TEST_CLOSE_READY", &ready)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let until = std::time::Instant::now() + Duration::from_secs(5);
        while !ready.exists()
            && std::time::Instant::now() < until
            && child.try_wait().unwrap().is_none()
        {
            std::thread::sleep(Duration::from_millis(20));
        }
        if !ready.exists() {
            let _ = child.kill();
            let _ = child.wait();
            panic!("test window did not start");
        }
        let control = GameControl::default();
        let fallback = control.clone();
        let (done, rx) = std::sync::mpsc::channel::<()>();
        let timeout = std::thread::spawn(move || {
            if rx.recv_timeout(Duration::from_secs(5)).is_err() {
                fallback.request_stop(true);
            }
        });
        control.request_stop(false);
        let status = wait(&mut child, &control).unwrap();
        let _ = done.send(());
        timeout.join().unwrap();
        assert!(status.success(), "game window was not closed gracefully");
    }
    #[test]
    fn force_stop_only_ends_the_owned_child() {
        #[cfg(windows)]
        let mut command = crate::minecraft::hidden_command("C:\\Windows\\System32\\ping.exe");
        #[cfg(windows)]
        command.args(["-n", "30", "127.0.0.1"]);
        #[cfg(not(windows))]
        let mut command = crate::minecraft::hidden_command("sleep");
        #[cfg(not(windows))]
        command.arg("30");
        command
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let mut owned = command.spawn().unwrap();
        let mut other = command.spawn().unwrap();
        let control = GameControl::default();
        control.request_stop(false);
        control.request_stop(true);
        assert!(!wait(&mut owned, &control).unwrap().success());
        assert!(control.stopped());
        assert!(other.try_wait().unwrap().is_none());
        other.kill().unwrap();
        other.wait().unwrap();
    }
}
