use nscl::{
    minecraft::{self, Downloader, SourceKind},
    store,
};
use std::sync::{Arc, atomic::AtomicBool};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut arguments = std::env::args().skip(1);
    let Some(command) = arguments.next() else {
        help();
        return Ok(());
    };
    if command == "--help" || command == "help" {
        help();
        return Ok(());
    }
    let mut id = None;
    let (mut settings, _) = store::load();
    let mut limit = 10usize;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--source" => {
                settings.source.kind = match arguments.next().ok_or("缺少下载源")?.as_str() {
                    "official" => SourceKind::Official,
                    "bmclapi" => SourceKind::Bmclapi,
                    "minguncle" => SourceKind::Minguncle,
                    "custom" => SourceKind::Custom,
                    _ => return Err("下载源应为 official/bmclapi/minguncle/custom".into()),
                }
            }
            "--mirror" => {
                settings.source.kind = SourceKind::Custom;
                settings.source.custom_base = arguments.next().ok_or("缺少镜像根地址")?;
            }
            "--no-fallback" => settings.source.fallback_to_official = false,
            "--game-dir" => settings.game_directory = arguments.next().ok_or("缺少游戏目录")?,
            "--java" => settings.java_path = arguments.next().ok_or("缺少 Java 路径")?,
            "--username" => settings.player_name = arguments.next().ok_or("缺少玩家名")?,
            "--demo" => settings.demo_mode = true,
            "--limit" => {
                limit = arguments
                    .next()
                    .ok_or("缺少列表长度")?
                    .parse()
                    .map_err(|_| "无效列表长度")?
            }
            value if !value.starts_with('-') && id.is_none() => id = Some(value.to_owned()),
            _ => return Err(format!("未知参数：{argument}")),
        }
    }
    let root = store::game_directory(&settings);
    if command == "installed" {
        for version in minecraft::installed_versions(&root) {
            println!(
                "{} · Java {} · {}",
                version.id, version.java_major, version.platform
            );
        }
        return Ok(());
    }
    if command == "launch" || command == "inspect" {
        let id = id.ok_or("请指定游戏版本")?;
        let plan = minecraft::prepare_launch(&root, &id, &settings)?;
        println!(
            "Java: {}\n游戏目录: {}\n日志: {}",
            plan.java.display(),
            plan.game_directory.display(),
            plan.log_path.display()
        );
        if command == "inspect" {
            for argument in plan.arguments {
                println!("{argument}");
            }
            return Ok(());
        }
        let status = minecraft::launch(&plan, |pid, path| {
            println!("游戏 PID={pid}, 日志={}", path.display())
        })?;
        if status != 0 {
            return Err(format!(
                "游戏退出码 {status}，日志：{}",
                plan.log_path.display()
            ));
        }
        return Ok(());
    }
    if command != "list" && command != "install" {
        return Err("未知命令，使用 --help 查看帮助。".into());
    }
    let downloader = Downloader::new(
        settings.source,
        Arc::new(AtomicBool::new(false)),
        Arc::new(|progress| {
            eprintln!(
                "{} [{}/{}] {:.1} MiB",
                progress.stage,
                progress.completed,
                progress.total,
                progress.bytes_downloaded as f64 / 1048576.0
            );
        }),
    )?;
    let manifest = minecraft::manifest(&downloader)?;
    if command == "list" {
        for version in manifest
            .versions
            .iter()
            .filter(|version| id.as_ref().is_none_or(|id| version.id.contains(id)))
            .take(limit)
        {
            println!("{}\t{}", version.id, version.kind);
        }
    } else {
        let id = id.ok_or("请指定游戏版本")?;
        let entry = manifest
            .versions
            .iter()
            .find(|entry| entry.id == id)
            .ok_or("版本列表中没有该版本")?;
        let installed = minecraft::install(&root, entry, &downloader)?;
        println!(
            "已安装 {}，需要 Java {}",
            installed.id, installed.java_major
        );
    }
    Ok(())
}

fn help() {
    println!(
        "NSCL 原版客户端工具\n命令：list [搜索词]、install <版本>、installed、inspect <版本>、launch <版本>\n参数：--source official|bmclapi|minguncle|custom、--mirror URL、--no-fallback、--game-dir PATH、--java PATH、--username NAME、--demo、--limit N\n离线身份不提供正版服务器认证。CLI 与桌面界面共用安装、校验及启动实现。"
    );
}
