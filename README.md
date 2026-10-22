# NorthStar Minecraft Launcher（NSCL）

**NSCL** 是面向 **Minecraft Java Edition** 的桌面启动器，使用 **Rust** 与 **eframe/egui** 构建原生窗口，提供游戏安装与启动、Microsoft 账号登录、独立实例管理，以及模组和整合包下载。

[CNB 主仓库](https://cnb.cool/JeffreyMing/NorthStar-Minecraft-Launcher) · [GitHub 镜像](https://github.com/JeffreyMing/NorthStar-Minecraft-Launcher) · [使用文档](#使用文档)

## 功能

| 功能 | 支持内容 |
| --- | --- |
| 游戏安装与启动 | 下载 Minecraft 原版客户端，浏览正式版、快照和历史版本，校验与修复安装文件 |
| 模组加载器 | 为所选 Minecraft 版本安装 Forge、Fabric 或 NeoForge，并创建对应实例 |
| Microsoft 账号 | 浏览器授权登录、多个账号切换、正版身份检查与登录续期 |
| 皮肤与角色 | 读取正版角色皮肤，预览并上传本地 PNG，支持标准/纤细手臂及可旋转的 3D 预览 |
| 独立实例 | 分别管理存档、模组和配置，支持重命名、归档与恢复 |
| Java 与内存 | 扫描或自动下载安装 Java，检查版本及架构，为实例单独配置 Java 和内存 |
| 模组下载与管理 | 搜索 Modrinth，显示 Logo，安装必需依赖，检查兼容更新并按方案更新或批量卸载 |
| 整合包 | 下载 Modrinth 整合包，导入或拖入本地 `.mrpack`，自动准备游戏与加载器 |
| 下载源 | Mojang 官方源、BMCLAPI、明叔镜像和自定义镜像，可配置失败时回退官方源 |
| 进度与诊断 | 后台下载、文件完整性校验、取消任务、兼容性检查与启动日志查看 |

## 运行要求

- **操作系统**：当前以 Windows 桌面环境为主要运行与验证平台；macOS、Linux 尚未完成发布验证。
- **Java**：需使用与所选 Minecraft 和加载器要求匹配的 Java。Windows 可在设置中自动下载安装 Eclipse Temurin JRE，也支持检测本机环境或手动指定路径。
- **网络**：安装游戏、下载资源和 Microsoft 登录需要访问相应服务。
- **游戏账号**：正版登录需要拥有 Minecraft Java Edition 授权的 Microsoft 账号。

## 获取与构建

当前仓库提供源码构建方式。在 Windows 上准备 Git、Rust stable 的 MSVC 工具链，以及 Microsoft C++ Build Tools 的「使用 C++ 的桌面开发」组件，然后运行：

```powershell
git clone https://cnb.cool/JeffreyMing/NorthStar-Minecraft-Launcher
cd NorthStar-Minecraft-Launcher
cargo build --release --locked --bin nscl
.\target\release\nscl.exe
```

发布构建产物为 `target/release/nscl.exe`。使用普通用户权限运行，配置与默认游戏数据保存在当前用户的数据目录；无需将游戏文件放在源码目录中。

## 快速开始

1. 打开「设置」，选择游戏文件目录和下载源，扫描、下载或指定 Java，按需调整内存后保存。下载 Java 后将路径留空即可自动匹配；「按当前实例选择」可填写所选实例要求的主版本。
2. 打开「账号」，点击「登录 Microsoft 账号」，在系统浏览器中完成授权后返回启动器。登录期间保持启动器打开，无需自行填写应用标识或回调地址。
3. 打开「下载 → Minecraft」，搜索并选择游戏版本，再选择「不安装加载器」或 Forge、Fabric、NeoForge 及其兼容版本，确认后点击「开始安装」。
4. 安装完成后回到「开始」，选择实例并点击「启动游戏」。通过「管理实例」可创建更多独立实例、配置 Java 和内存，或归档与恢复已有实例。

### 安装模组

打开「下载 → Mod」，选择已经安装加载器的目标实例，搜索资源并点击「查看版本」。选择兼容版本后点击「查看方案」，确认模组及必需依赖，再点击「安装模组及依赖」。可通过「检查兼容性」查看已登记文件的版本、依赖和完整性问题。

点击「管理已装模组」查看文件状态和兼容更新，选择「查看更新方案」后确认安装。卸载时勾选模组并点击「卸载所选」，核对依赖检查及文件列表后确认；存档和配置保留。整合包自带与手动添加的未独立登记 JAR 会单独列出，可打开实例目录管理。

### 安装整合包

打开「下载 → 整合包」，搜索并选择发布版本，或填写、拖入本地 `.mrpack` 文件。确认所需游戏版本、加载器与可选客户端文件后，点击「安装整合包」。启动器会创建新的独立实例，完成后可从开始页选择启动。

模组和整合包列表、版本页及安装确认页显示项目 Logo。图片在后台加载并缓存，缺图或加载失败时使用占位图。

### 管理角色皮肤

在首页点击「更换皮肤」，或进入「账号 → 皮肤与预览」。支持拖动旋转角色、双击复位，选择标准或纤细手臂，并预览本地 PNG。点击「上传到当前正版账号」后才会修改在线皮肤；「刷新账号皮肤」可重新读取当前账号的显示。登录与切换账号时会自动更新角色预览。

## 下载源

在「设置」选择游戏下载源：

| 下载源 | 地址或要求 |
| --- | --- |
| Mojang 官方源 | 默认选项，按游戏元数据访问官方服务 |
| BMCLAPI（bangbang93） | `https://bmclapi2.bangbang93.com` |
| 明叔镜像 | `https://mirror.minguncle.cn` |
| 自定义镜像 | 填写兼容 BMCLAPI 路径映射的 HTTP/HTTPS 根地址 |

镜像失败时默认回退官方源，可在设置中关闭。自定义地址应为镜像根地址，不能填写普通网页或单个文件的下载链接；接口要求见[原版客户端文档](docs/vanilla-client.md#下载源)和[加载器文档](docs/loaders.md#镜像)。

游戏下载源用于 Minecraft 与加载器依赖。Modrinth 模组、整合包及 Logo 使用平台提供的资源地址；Microsoft、Xbox 和 Minecraft 身份认证直接访问官方认证服务。

## 数据与账号

Windows 发布构建默认使用 `%LOCALAPPDATA%\cool.cnb.jeffreyming.nscl\`：

| 路径 | 内容 |
| --- | --- |
| `settings.json` | 启动器偏好、Java、内存、游戏下载源与目录设置 |
| `nscl-accounts.json` | 玩家公开资料与当前账号选择，不包含登录令牌 |
| `cache/project-icons/` | 模组与整合包 Logo 缩略图缓存 |
| `cache/skins/` | 角色皮肤与模型类型缓存 |
| `java/` | 自动安装的 Java 运行时与下载缓存 |
| `minecraft/` | 默认游戏文件目录，可在设置中更改 |

游戏文件目录中的 `versions/`、`libraries/` 和 `assets/` 保存共享游戏文件；`instances/<实例 ID>/` 分别保存各实例的存档、模组与配置。归档的实例移入 `archived-instances/`，可在「管理实例」中恢复。归档保存在同一游戏文件目录，重要存档仍需单独备份。

Windows 登录刷新令牌保存在系统凭据管理器中，不写入设置或账号 JSON。系统凭据库不可用时，启动器会提示登录仅对当前会话有效。移除账号会删除本机保存的对应凭据。账号与数据管理详见[账号、Java 与实例管理](docs/accounts-and-instances.md)。

## 常见问题

| 问题 | 处理方式 |
| --- | --- |
| 找不到合适的 Java | 在设置中扫描 Java，或安装并指定游戏要求的主版本与架构；实例单独配置的 Java 优先于全局设置 |
| 无法打开登录浏览器 | 使用账号页的「重新打开浏览器」或「复制登录链接」；确认系统已设置默认浏览器，并让浏览器与启动器以普通用户权限运行 |
| 下载或文件校验失败 | 检查网络与下载源配置，按需切换来源后重试；已通过校验的文件会复用，损坏文件会重新下载 |
| 模组或整合包无法安装 | 查看兼容性提示，确认游戏版本、加载器和依赖要求；CurseForge 包不能通过改名为 `.mrpack` 导入 |
| 游戏启动后退出 | 在开始页打开「查看日志」，结合实例 Java、内存和模组配置排查 |
| Fabric 提示低完整性权限或首次映射 JAR 只读 | 退出启动器，将 EXE 复制到普通用户目录后重新打开；沙箱开发目录中的 EXE 可能继承低完整性标签。无需重装游戏，详见[加载器文档](docs/loaders.md#fabric-首次启动与-windows-权限) |

游戏启动日志位于 `<游戏文件目录>/instances/<实例 ID>/logs/nscl-launch.log`；Forge / NeoForge 安装日志位于 `<游戏文件目录>/.nscl/installers/`。反馈问题时请附启动器版本、操作系统、Minecraft 与加载器版本、复现步骤及已脱敏的日志，不要提交令牌或浏览器授权链接。

## 当前支持范围

- 模组与整合包资源目前接入 Modrinth，支持 `.mrpack`；尚未接入 CurseForge、Quilt。已登记模组支持检查较新兼容正式版、确认后更新和依赖检查后卸载。
- 兼容性检查依据资源发布信息、依赖与整合包清单，不保证所有第三方模组组合都能正常运行。
- 皮肤上传需使用当前正版账号；本地预览不修改服务器上的皮肤，离线身份不提供在线皮肤同步。
- Java 自动安装目前支持 Windows；暂未集成安装向导或启动器自动更新。
- 离线身份用于本地测试，不能访问要求正版认证的服务器；正版身份验证失败不会自动切换为离线身份。

## 使用文档

| 文档 | 内容 |
| --- | --- |
| [原版客户端安装与启动](docs/vanilla-client.md) | 游戏安装、镜像接口、文件修复和命令行使用 |
| [Forge、Fabric 与 NeoForge](docs/loaders.md) | 加载器安装、Java 要求和安装日志 |
| [账号、Java 与实例管理](docs/accounts-and-instances.md) | 登录、凭据保存、Java 配置和实例归档 |
| [模组与整合包](docs/mods-and-modpacks.md) | 资源下载、Logo、依赖解析、整合包格式与兼容性范围 |

## 开发与贡献

界面与业务逻辑均使用 Rust，UI 框架为 eframe/egui。网络请求使用 reqwest，本地配置使用 serde / serde_json；桌面界面与 `nscl-cli` 共用游戏安装、账号和实例核心。

提交改动前可执行：

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
```

浏览器授权、系统凭据库及真实资源下载等集成测试需按需显式运行，具体说明见对应功能文档。

欢迎在 [CNB 主仓库](https://cnb.cool/JeffreyMing/NorthStar-Minecraft-Launcher)反馈问题、提出建议或提交改进。CNB 为主仓库，GitHub 为同步镜像；同步配置见 [CNB → GitHub 自动同步](docs/github-sync.md)。

## 项目声明与许可证

NSCL 是独立的非官方项目，与 Mojang Studios 或 Microsoft 无隶属或背书关系。Minecraft 及相关名称、商标归其各自权利人所有。

仓库目前尚未指定开源许可证，也未提供 `LICENSE` 文件。
