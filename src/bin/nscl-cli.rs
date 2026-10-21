use nscl::{
    accounts::{AccountList, AuthClient, CredentialStore, SystemVault},
    instances::{self, Instance},
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
            "--username" => {
                settings.player_name = arguments.next().ok_or("缺少玩家名")?;
                settings.online_mode = false;
            }
            "--offline" => settings.online_mode = false,
            "--demo" => {
                settings.demo_mode = true;
                settings.online_mode = false;
            }
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
    if command == "java" {
        for runtime in nscl::java::discover() {
            println!("{}\n{}", runtime.label(), runtime.path.display());
        }
        return Ok(());
    }
    if command == "instances" {
        for instance in instances::list(&root) {
            println!(
                "{}\t{}\t{}",
                instance.id, instance.name, instance.version_id
            );
        }
        return Ok(());
    }
    if command == "login" {
        let session = AuthClient::new()?.login(
            &settings.microsoft_client_id,
            &AtomicBool::new(false),
            |code| {
                println!(
                    "请自行打开 {} 并输入设备代码：{}",
                    code.verification_uri, code.user_code
                );
            },
            |stage| eprintln!("{stage}"),
        )?;
        SystemVault.write(&session.account.id, &session.credential)?;
        let mut list = AccountList::load()?;
        let name = session.account.name.clone();
        list.upsert(session.account);
        list.save()?;
        let (mut saved, _) = store::load();
        saved.online_mode = true;
        store::save(&saved)?;
        println!("{name} 已登录并通过 Java 版授权验证。");
        return Ok(());
    }
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
        let id = id.ok_or("请指定实例 ID 或游戏版本")?;
        let instance = instances::list(&root)
            .into_iter()
            .find(|i| i.id == id)
            .unwrap_or_else(|| Instance {
                id: id.clone(),
                name: id.clone(),
                version_id: id,
                ..Default::default()
            });
        let identity = if settings.online_mode {
            let list = AccountList::load()?;
            let account = list
                .current()
                .ok_or("请先使用 login 登录 Microsoft 账号，或明确指定 --offline。")?;
            let session =
                AuthClient::new()?.resume(account, None, &SystemVault, &AtomicBool::new(false))?;
            minecraft::LaunchIdentity::microsoft(&session)
        } else {
            minecraft::LaunchIdentity::offline(&settings.player_name)
        };
        let plan = minecraft::prepare_instance_launch(&root, &instance, &settings, &identity)?;
        println!(
            "Java: {}\n游戏目录: {}\n日志: {}",
            plan.java.display(),
            plan.game_directory.display(),
            plan.log_path.display()
        );
        if command == "inspect" {
            for argument in plan.arguments {
                if identity.online {
                    println!("{}", argument.replace(&identity.access_token, "<redacted>"));
                } else {
                    println!("{argument}");
                }
            }
            return Ok(());
        }
        let status = minecraft::launch(&plan, |pid, path| {
            let mut instance = instance.clone();
            instance.last_played = Some(nscl::accounts::now());
            if let Err(error) = instances::save(&root, &instance) {
                eprintln!("实例记录保存失败：{error}");
            }
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
        "NSCL 客户端工具\n命令：list [搜索词]、install <版本>、installed、instances、java、login、inspect <实例或版本>、launch <实例或版本>\n参数：--source official|bmclapi|minguncle|custom、--mirror URL、--no-fallback、--game-dir PATH、--java PATH、--offline、--username NAME、--demo、--limit N\nCLI 使用已选择的身份，--offline / --username 明确切换为离线测试。inspect 会隐藏正版令牌。"
    );
}
