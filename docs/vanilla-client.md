# 原版客户端安装与启动

## 桌面使用流程

1. 在「设置」选择游戏文件目录、下载源和 Java 路径，然后保存。Java 路径留空时，会检查 `JAVA_HOME`、`PATH` 以及常见 JDK 安装目录。
2. 打开「下载 → Minecraft」，页面会自动加载并直接显示 Minecraft 版本。可搜索版本号，默认仅显示正式版，也可以显示快照和历史测试版；加载失败时可点击「重新加载」。
3. 点击「选择版本」，保留「不安装加载器」，再点击「开始安装」。也可在此选择 Forge、Fabric 或 NeoForge 及其兼容版本。确认前不会开始下载；可点击「返回版本列表」重新选择游戏版本。后台下载版本 JSON、客户端 JAR、当前系统所需的依赖库、资源索引、资源对象和日志配置，校验 SHA-1 与文件长度，并准备旧式原生库及资源目录。
4. 在「账号」登录 Microsoft 账号并完成正版授权，或明确选择离线测试身份。安装完成后回到「开始」，选择实例并点击「启动游戏」。启动前会验证正版身份、再次校验文件，选择版本要求的 Java，并直接创建 Java 进程。
5. 游戏退出码会反馈到启动器；运行中或退出后可通过「查看日志」查看最近的启动日志。

正版登录、多账号切换、凭据保存和实例独立配置见 [账号、Java 与实例管理](accounts-and-instances.md)。离线身份不能访问需要正版认证的服务器，试玩模式仅对离线测试生效；正版身份验证失败不会自动切换到离线身份。皮肤上传与渲染尚未实现。

## 下载源

| 选项 | 根地址 | 说明 |
| --- | --- | --- |
| Mojang 官方源 | 按 Mojang 元数据使用官方域名 | 默认选项 |
| BMCLAPI（bangbang93） | `https://bmclapi2.bangbang93.com` | 设置页的下载源选项中标注来源，接口参见 [BMCLAPI 官方文档](https://bmclapidoc.bangbang93.com/) |
| mirror.minguncle.cn | `https://mirror.minguncle.cn` | 采用兼容 BMCLAPI 的路径映射 |
| 自定义镜像 | 用户填写 HTTP/HTTPS 根地址 | 支持带路径前缀的根地址，例如 `https://example.com/minecraft` |

自定义镜像需兼容下列路径，而不是任意网页或单独的下载链接：

| 资源 | 官方地址 | 镜像地址 |
| --- | --- | --- |
| 版本列表 | `https://piston-meta.mojang.com/mc/game/version_manifest_v2.json` | `{base}/mc/game/version_manifest_v2.json` |
| 版本 JSON / 资源索引 | Mojang 元数据中的 `/v1/packages/...` | `{base}/v1/packages/...` |
| 客户端 / 日志配置 | Mojang 下载域名的 `/v1/objects/...` | `{base}/v1/objects/...` |
| Java 库 | `https://libraries.minecraft.net/{path}` | `{base}/maven/{path}` |
| 游戏资源对象 | `https://resources.download.minecraft.net/{prefix}/{hash}` | `{base}/assets/{prefix}/{hash}` |

默认在镜像请求或校验失败时回退到对应官方 URL，可在设置中关闭。元数据内已改写为内置镜像地址的 URL 会先恢复为官方路径，再按当前下载源路由。镜像地址不接受嵌入账号密码、查询参数或片段。

HTTP 请求使用系统 TLS 校验和系统代理设置；本地回环镜像直接连接。每个文件最多尝试两次，再尝试回退源。下载在后台进行，支持取消；取消会在网络操作返回后生效。

## 文件结构与修复

```text
<游戏文件目录>/
  versions/<version>/<version>.json
  versions/<version>/<version>.jar
  versions/<version>/nscl-instance.json
  versions/<version>/natives/<os>-<arch>/
  libraries/
  assets/indexes/
  assets/objects/
  assets/log_configs/
  assets/virtual/                 # 部分旧版需要
  instances/<instance-id>/       # 实例独立的存档、模组与配置目录
  instances/<instance-id>/nscl-instance.json
  instances/<instance-id>/logs/nscl-launch.log
  archived-instances/<archive-id>/ # 可恢复的完整实例归档
```

仅在全部文件校验和准备工作成功后写入 `nscl-instance.json` 完成标记。未完成的首次安装不会出现在首页。

文件先写入 `.part`，校验通过后再替换目标文件。重试安装会复用已通过 SHA-1 校验的文件，损坏或缺失的文件会重新下载。「校验 / 修复」使用相同流程，不删除存档或配置。目前不支持单个未完成文件的字节断点续传；中断的文件会从头下载。

元数据中的相对路径和原生库 ZIP 条目会检查目录穿越，原生库不解压符号链接和 `META-INF/`，并限制解压体积。

## Java 与版本兼容性

- 按版本 JSON 的 `javaVersion.majorVersion` 选择相同主版本的 Java；未声明的历史版本按 Java 8 处理。当前不会自动下载 JRE。
- 设置页可扫描系统 Java、检测手动路径并查看发行版及架构；实例可覆盖全局 Java 与内存，也可以单独启用自动匹配。检测在后台运行，单个探测有超时限制。
- 支持现代 `arguments.jvm/game`，以及旧式 `minecraftArguments`。按顺序应用操作系统、架构、系统版本和功能规则。
- 支持旧版分类器原生库解压、`virtual` / `map_to_resources` 资源布局，以及官方日志参数。
- 启动使用独立参数数组，不拼接 shell 命令，因此游戏目录和 Java 路径可以包含空格。
- Forge/Fabric/NeoForge 的继承元数据不属于此功能；没有标准客户端入口或完整下载信息的早期实验版本会返回明确错误。
- 安装包包含当前系统所需的文件，切换系统或架构后需重新安装/修复。游戏运行时暂停安装与修复，避免修改已使用的文件。

## 命令行与开发验证

`nscl-cli` 与桌面界面调用同一套核心代码；`login` 会保存账号与正版身份选项，其他参数仅对当前命令生效：

```powershell
cargo run --bin nscl-cli -- list 1.20.1 --source official --limit 5
cargo run --bin nscl-cli -- install 1.20.1 --source bmclapi --game-dir "H:\Minecraft"
cargo run --bin nscl-cli -- install 1.20.1 --mirror https://example.com --game-dir "H:\Minecraft"
cargo run --bin nscl-cli -- installed --game-dir "H:\Minecraft"
cargo run --bin nscl-cli -- instances --game-dir "H:\Minecraft"
cargo run --bin nscl-cli -- java
cargo run --bin nscl-cli -- login
cargo run --bin nscl-cli -- inspect 1.20.1 --game-dir "H:\Minecraft"
cargo run --bin nscl-cli -- launch 1.20.1 --game-dir "H:\Minecraft" --username NSCLPlayer
cargo test --lib
cargo clippy --all-targets -- -D warnings
```

`inspect` 校验文件并生成启动计划，不启动游戏，输出会隐藏正版令牌；`launch` / `inspect` 接受实例 ID 或旧版默认实例的游戏版本号。CLI 默认使用已选择的身份，`--offline` / `--username` 明确切换离线；`--java` 可指定全局 Java（实例单独配置优先），`--demo` 启用离线试玩，`--no-fallback` 禁用官方回退。

本地模拟服务器测试覆盖下载与安装、认证轮询与续期、正版授权和启动参数、Java 架构、实例配置与存档归档恢复。进程测试使用测试子进程，不把它视为真实游戏启动验证；模拟认证不代表真实账号或应用权限已验证。

桌面调试预览可用 `NSCL_PREVIEW_GAME_DIR` 临时指定已有安装的目录，不需要覆盖正常设置文件。若显式配置了 `NSCL_SCREENSHOT`，调试窗口按 F12 可导出当前画面。
