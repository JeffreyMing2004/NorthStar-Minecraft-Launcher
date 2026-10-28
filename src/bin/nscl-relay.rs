use nscl::{
    accounts::AccountKind,
    multiplayer::{AccessList, issue_host, issue_member, server::RelayServer},
};
use std::{
    net::SocketAddr,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        return Err("用法：nscl-relay init <凭证文件> | issue-host <路径> [数量] | revoke-host <路径> <凭证编号> | list <路径> | import-frpc <原始配置> <私有JSON路径> | probe-frpc <私有JSON路径> | install-frpc <安装目录> | serve <凭证文件> [127.0.0.1:17630] [frpc私有JSON路径]；旧版账号凭证命令 add-offline / add-microsoft / remove 继续兼容。".into());
    }
    let path = Path::new(&args[1]);
    if args[0] == "install-frpc" && args.len() == 2 {
        let executable = nscl::multiplayer::frpc::download::install(
            path,
            &Arc::new(AtomicBool::new(false)),
            &|message| println!("{message}"),
        )?;
        println!("frpc 程序：{}", executable.display());
        return Ok(());
    }
    if args[0] == "probe-frpc" {
        let catalog = nscl::multiplayer::frpc::Catalog::load(path)?;
        let nodes: Vec<_> = catalog.nodes.into_iter().map(|n| n.info).collect();
        let results =
            nscl::multiplayer::frpc::measure_nodes(&nodes, &Arc::new(AtomicBool::new(false)))?;
        for result in results {
            println!("{} · TCP {} ms", result.node.name, result.latency_ms);
        }
        return Ok(());
    }
    if args[0] == "import-frpc" && args.len() == 3 {
        let text = std::fs::read_to_string(path).map_err(|_| "无法读取 frpc 原始配置。")?;
        let catalog = nscl::multiplayer::frpc::Catalog::import(&text)?;
        nscl::store::write_json(Path::new(&args[2]), &catalog)?;
        println!(
            "已导入 {} 个节点、{} 条隧道；私有配置不会输出到终端。",
            catalog.nodes.len(),
            catalog.nodes.iter().map(|n| n.proxies.len()).sum::<usize>()
        );
        return Ok(());
    }
    if args[0] == "serve" {
        let address: SocketAddr = args
            .get(2)
            .map(String::as_str)
            .unwrap_or("127.0.0.1:17630")
            .parse()
            .map_err(|_| "监听地址无效。")?;
        if !address.ip().is_loopback() {
            return Err("验证服务仅允许监听回环地址，请通过服务器上的 HTTPS 反向代理发布。".into());
        }
        let mut service = RelayServer::new(path)?;
        if let Some(catalog) = args.get(3) {
            service = service.with_frpc_catalog(Path::new(catalog))?;
        }
        let service = Arc::new(service);
        let server = tiny_http::Server::http(address).map_err(|_| "无法监听联机服务端口。")?;
        println!(
            "联机服务监听 {address}，最多同时开放 28 个 frpc 房间；加入者不占房主名额。请配置 HTTPS 后接入启动器。"
        );
        return service.serve(server, Arc::new(AtomicBool::new(false)));
    }
    if args[0] == "list" {
        let list = AccessList::load(path)?;
        println!(
            "独立房主凭证 {} 个，旧版账号凭证 {} 个；同时开房上限 28 个。",
            list.host_keys.len(),
            list.members.len()
        );
        for key in &list.host_keys {
            println!("房主凭证编号：{}", key.id);
        }
        for member in list.members {
            println!(
                "{} {} {:?}",
                member.identity.id, member.identity.name, member.identity.kind
            );
        }
        return Ok(());
    }
    // Serialize local administrator edits. Public HTTP requests have no enrollment endpoint.
    let lock_path = path.with_extension("lock");
    let lock = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .map_err(|_| "凭证文件正在编辑，或所在目录不存在。")?;
    let result = edit(&args, path);
    drop(lock);
    let _ = std::fs::remove_file(lock_path);
    result
}

fn edit(args: &[String], path: &Path) -> Result<(), String> {
    if args[0] == "init" {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|_| "无法创建凭证文件，已有文件不会覆盖。")?;
        file.write_all(b"{\"members\": [], \"host_keys\": []}\n")
            .map_err(|_| "写入凭证文件失败。")?;
        return Ok(());
    }
    let mut list = AccessList::load(path)?;
    match args[0].as_str() {
        "issue-host" if args.len() == 2 || args.len() == 3 => {
            let count = args
                .get(2)
                .map(|v| v.parse::<usize>())
                .transpose()
                .map_err(|_| "凭证数量应为 1–1000。")?
                .unwrap_or(1);
            if !(1..=1000).contains(&count) {
                return Err("凭证数量应为 1–1000。".into());
            }
            let issued = (0..count)
                .map(|_| issue_host(&mut list))
                .collect::<Result<Vec<_>, _>>()?;
            nscl::store::write_json(path, &list)?;
            println!(
                "房主凭证已生成，可直接发放，无需登记玩家名或 UUID。请私下交付开房码（仅显示本次）："
            );
            for (id, token) in issued {
                println!("编号：{id}\n开房码：{token}");
            }
        }
        "revoke-host" if args.len() == 3 => {
            let old = list.host_keys.len();
            list.host_keys.retain(|key| key.id != args[2]);
            if old == list.host_keys.len() {
                return Err("没有该编号的房主凭证。".into());
            }
            nscl::store::write_json(path, &list)?;
            println!("已撤销该开房码，其房间将关闭并释放隧道。");
        }
        "add-offline" if args.len() == 3 => {
            let token = issue_member(&mut list, AccountKind::Offline, "", &args[2])?;
            nscl::store::write_json(path, &list)?;
            println!(
                "已登记账号。请私下交付以下联机凭证（仅显示本次，勿放入 Git 或公开分享）：\n{token}"
            );
        }
        "add-microsoft" if args.len() == 4 => {
            let token = issue_member(&mut list, AccountKind::Microsoft, &args[2], &args[3])?;
            nscl::store::write_json(path, &list)?;
            println!(
                "已登记账号。请私下交付以下联机凭证（仅显示本次，勿放入 Git 或公开分享）：\n{token}"
            );
        }
        "remove" if args.len() == 3 => {
            let id = args[2].replace('-', "").to_ascii_lowercase();
            let old = list.members.len();
            list.members.retain(|m| m.identity.id != id);
            if old == list.members.len() {
                return Err("白名单中没有该 UUID。".into());
            }
            nscl::store::write_json(path, &list)?;
            println!("已撤销该账号的联机权限。");
        }
        _ => return Err("未知操作或参数数量错误。".into()),
    }
    Ok(())
}
