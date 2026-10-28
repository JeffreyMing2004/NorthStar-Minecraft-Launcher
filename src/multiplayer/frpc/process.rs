use super::Route;
use crate::multiplayer::Result;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Failure {
    Busy,
    Authentication,
    Network,
    Cancelled,
    Process,
}
impl Failure {
    pub fn message(self) -> String {
        match self {
            Self::Busy => "该 frpc 隧道已被占用，正在尝试其他隧道。",
            Self::Authentication => "frpc 节点认证失败，请联系管理员核对节点配置。",
            Self::Network => "frpc 节点连接超时或已断开。",
            Self::Cancelled => "操作已取消。",
            Self::Process => "frpc 启动失败，请检查程序路径和版本。",
        }
        .into()
    }
}

enum Event {
    Ready,
    Failed(Failure),
}

pub(crate) struct FrpcProcess {
    child: Child,
    config: PathBuf,
    #[cfg(windows)]
    _job: std::os::windows::io::OwnedHandle,
}
impl FrpcProcess {
    pub fn start(
        executable: &Path,
        route: &Route,
        port: u16,
        directory: &Path,
        cancel: &Arc<AtomicBool>,
    ) -> std::result::Result<Self, Failure> {
        if !executable.is_file() {
            return Err(Failure::Process);
        }
        let mut command = crate::minecraft::hidden_command(executable);
        #[cfg(test)]
        if executable == std::env::current_exe().unwrap() {
            command.args([
                "--ignored",
                "--exact",
                "multiplayer::frpc::tests::fake_frpc_process",
                "--nocapture",
            ]);
        }
        Self::spawn(&mut command, route, port, directory, cancel)
    }
    pub(super) fn spawn(
        command: &mut Command,
        route: &Route,
        port: u16,
        directory: &Path,
        cancel: &Arc<AtomicBool>,
    ) -> std::result::Result<Self, Failure> {
        if cancel.load(Ordering::Relaxed) {
            return Err(Failure::Cancelled);
        }
        fs::create_dir_all(directory).map_err(|_| Failure::Process)?;
        let config = directory.join(format!("frpc-{}.json", uuid::Uuid::new_v4().simple()));
        let bytes = route.config(port).map_err(|_| Failure::Process)?;
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&config).map_err(|_| Failure::Process)?;
        if file
            .write_all(&bytes)
            .and_then(|()| file.sync_all())
            .is_err()
        {
            drop(file);
            let _ = fs::remove_file(&config);
            return Err(Failure::Process);
        }
        drop(file);
        // No credential is passed through the process command line or exposed in errors.
        #[cfg(test)]
        let fixture = command.get_program() == std::env::current_exe().unwrap().as_os_str();
        #[cfg(not(test))]
        let fixture = false;
        if !fixture {
            command.arg("-c").arg(&config);
        }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(test)]
        if command.get_program() == std::env::current_exe().unwrap().as_os_str() {
            command.env("NSCL_TEST_FRPC_CONFIG", &config);
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(_) => {
                let _ = fs::remove_file(&config);
                return Err(Failure::Process);
            }
        };
        #[cfg(windows)]
        let job = match job(&child) {
            Ok(job) => job,
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = fs::remove_file(&config);
                return Err(Failure::Process);
            }
        };
        let (sender, receiver) = mpsc::sync_channel(16);
        for reader in [
            child
                .stdout
                .take()
                .map(|r| Box::new(r) as Box<dyn Read + Send>),
            child
                .stderr
                .take()
                .map(|r| Box::new(r) as Box<dyn Read + Send>),
        ]
        .into_iter()
        .flatten()
        {
            let (sender, name) = (sender.clone(), route.proxy.name.clone());
            std::thread::spawn(move || read_events(reader, &name, sender));
        }
        let mut process = Self {
            child,
            config,
            #[cfg(windows)]
            _job: job,
        };
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(Failure::Cancelled);
            }
            if process
                .child
                .try_wait()
                .map_err(|_| Failure::Process)?
                .is_some()
            {
                return Err(Failure::Process);
            }
            match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(Event::Ready) => return Ok(process),
                Ok(Event::Failed(failure)) => return Err(failure),
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(Failure::Process),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            if Instant::now() >= deadline {
                return Err(Failure::Network);
            }
        }
    }
    pub fn running(&mut self) -> bool {
        self.child.try_wait().is_ok_and(|status| status.is_none())
    }
}
impl Drop for FrpcProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_file(&self.config);
    }
}

fn read_events(mut reader: Box<dyn Read + Send>, name: &str, sender: mpsc::SyncSender<Event>) {
    let mut bytes = [0; 1024];
    let mut line = Vec::new();
    while let Ok(n) = reader.read(&mut bytes) {
        if n == 0 {
            break;
        }
        for &byte in &bytes[..n] {
            if byte == b'\n' {
                classify(&line, name, &sender);
                line.clear();
            } else if line.len() < 4096 {
                line.push(byte);
            }
        }
    }
    if !line.is_empty() {
        classify(&line, name, &sender);
    }
}
fn classify(bytes: &[u8], name: &str, sender: &mpsc::SyncSender<Event>) {
    let line = String::from_utf8_lossy(bytes).to_ascii_lowercase();
    let event = if line.contains("start proxy success") && line.contains(&name.to_ascii_lowercase())
    {
        Some(Event::Ready)
    } else if line.contains("start proxy error")
        || line.contains("port already used")
        || line.contains("proxy name") && line.contains("already")
    {
        Some(Event::Failed(Failure::Busy))
    } else if line.contains("authentication failed")
        || line.contains("authorization failed")
        || line.contains("token in login")
        || line.contains("user") && line.contains("not found")
    {
        Some(Event::Failed(Failure::Authentication))
    } else {
        None
    };
    if let Some(event) = event {
        let _ = sender.try_send(event);
    }
}

#[cfg(windows)]
fn job(child: &Child) -> Result<std::os::windows::io::OwnedHandle> {
    use std::{
        mem::size_of,
        os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    };
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };
    // SAFETY: null attributes/name create an unnamed job owned by this handle.
    let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if handle.is_null() {
        return Err("无法创建 frpc 进程管理对象。".into());
    }
    // SAFETY: a successful CreateJobObjectW transfers a live owned handle.
    let job = unsafe { OwnedHandle::from_raw_handle(handle) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    // SAFETY: all handles and the configuration buffer remain valid during both calls.
    if unsafe {
        SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    } == 0
        || unsafe { AssignProcessToJobObject(job.as_raw_handle(), child.as_raw_handle()) } == 0
    {
        return Err("无法接管 frpc 进程。".into());
    }
    Ok(job)
}
