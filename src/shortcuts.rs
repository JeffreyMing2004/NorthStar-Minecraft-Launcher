use crate::{
    instances,
    minecraft::{Result, validate_id},
};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};
#[derive(Default, Debug, PartialEq, Eq)]
pub struct Startup {
    pub instance: Option<String>,
    pub root: Option<PathBuf>,
}
pub fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Startup> {
    let mut result = Startup::default();
    let mut args = arguments.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--launch-instance" {
            let id = args
                .next()
                .and_then(|s| s.into_string().ok())
                .ok_or("缺少快捷启动实例 ID。")?;
            validate_id(&id)?;
            if result.instance.replace(id).is_some() {
                return Err("重复的快捷启动实例参数。".into());
            }
        } else if arg == "--game-dir" {
            result.root = Some(PathBuf::from(args.next().ok_or("缺少快捷启动游戏目录。")?));
        } else if arg == "--renderer" {
            args.next().ok_or("缺少渲染模式。")?;
        } else if !arg.to_string_lossy().starts_with("--renderer=") {
            return Err("GUI 参数只支持 --launch-instance、--game-dir 和 --renderer。".into());
        }
    }
    Ok(result)
}
pub fn arguments(root: &Path, id: &str) -> Result<Vec<String>> {
    validate_id(id)?;
    Ok(vec![
        "--launch-instance".into(),
        id.into(),
        "--game-dir".into(),
        root.to_string_lossy().into_owned(),
    ])
}
#[cfg(any(windows, test))]
fn windows_quote(value: &str) -> String {
    let mut result = String::from("\"");
    let mut slashes = 0;
    for c in value.chars() {
        if c == '\\' {
            slashes += 1;
            continue;
        }
        if c == '"' {
            result.push_str(&"\\".repeat(slashes * 2 + 1));
        } else {
            result.push_str(&"\\".repeat(slashes));
        }
        slashes = 0;
        result.push(c);
    }
    result.push_str(&"\\".repeat(slashes * 2));
    result.push('"');
    result
}

pub fn create(root: &Path, id: &str, destination: &Path) -> Result<PathBuf> {
    let root =
        std::fs::canonicalize(instances::backup::checked(root)?).map_err(|e| e.to_string())?;
    if !instances::list(&root).iter().any(|i| i.id == id) {
        return Err("快捷方式对应的实例不存在。".into());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let args = arguments(&root, id)?;
    for p in destination.ancestors() {
        instances::reject_link(p)?;
    }
    if destination.exists() {
        return Err("目标文件已存在，不会覆盖。".into());
    }
    #[cfg(windows)]
    {
        if !destination
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("lnk"))
        {
            return Err("Windows 快捷方式请保存为 .lnk。".into());
        }
        let script = "$ErrorActionPreference='Stop'; $w=New-Object -ComObject WScript.Shell; $s=$w.CreateShortcut($env:NSCL_LINK_DEST); $s.TargetPath=$env:NSCL_LINK_EXE; $s.Arguments=$env:NSCL_LINK_ARGS; $s.WorkingDirectory=[System.IO.Path]::GetDirectoryName($env:NSCL_LINK_EXE); $s.Description='NorthStar Minecraft Launcher'; $s.Save()";
        let output = crate::minecraft::hidden_command("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle",
                "Hidden",
                "-Command",
                script,
            ])
            .env("NSCL_LINK_DEST", destination)
            .env("NSCL_LINK_EXE", &exe)
            .env(
                "NSCL_LINK_ARGS",
                args.iter()
                    .map(|a| windows_quote(a))
                    .collect::<Vec<_>>()
                    .join(" "),
            )
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err("创建快捷方式失败，请选择可写的完整 .lnk 保存路径。".into());
        }
    }
    #[cfg(not(windows))]
    {
        use std::io::Write;
        fn quote(s: &str) -> String {
            format!("'{}'", s.replace('\'', "'\\''"))
        }
        let command = std::iter::once(exe.to_string_lossy().into_owned())
            .chain(args)
            .map(|s| quote(&s))
            .collect::<Vec<_>>()
            .join(" ");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .map_err(|e| e.to_string())?;
        file.write_all(format!("#!/bin/sh\nexec {command}\n").as_bytes())
            .map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(destination.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_keeps_instance_and_directory_boundaries_and_rejects_missing_or_duplicate_ids() {
        let args = arguments(Path::new("H:/game directory with spaces"), "instance-test").unwrap();
        let parsed = parse(args.into_iter().map(OsString::from)).unwrap();
        assert_eq!(parsed.instance.as_deref(), Some("instance-test"));
        assert_eq!(
            parsed.root,
            Some(PathBuf::from("H:/game directory with spaces"))
        );
        assert!(parse(["--launch-instance", "../escape"].map(OsString::from)).is_err());
        assert!(
            parse(["--launch-instance", "a", "--launch-instance", "b"].map(OsString::from))
                .is_err()
        );
        assert!(parse([OsString::from("--game-dir")]).is_err());
        assert_eq!(
            windows_quote("H:\\game with spaces\\"),
            "\"H:\\game with spaces\\\\\""
        );
        assert_eq!(windows_quote("has\"quote"), "\"has\\\"quote\"");
    }
}
