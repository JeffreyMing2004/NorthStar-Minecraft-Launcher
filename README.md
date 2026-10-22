# NorthStar Minecraft Launcher

**NSCL** 是一款使用 Rust 与 eframe/egui 构建的 Minecraft Java Edition 桌面启动器。原生窗口、单一可执行文件、无需额外运行库。

[下载](https://github.com/JeffreyMing2004/NorthStar-Minecraft-Launcher/releases) · [使用文档](#使用文档) · [问题反馈](https://github.com/JeffreyMing2004/NorthStar-Minecraft-Launcher/issues) · [CNB 镜像](https://cnb.cool/JeffreyMing/NorthStar-Minecraft-Launcher)

## 功能

- **安装与启动**：正式版、快照与历史版本，下载、校验与修复游戏文件
- **模组加载器**：Forge、Fabric、NeoForge，按所选版本自动匹配安装
- **账号**：Microsoft 浏览器授权、离线账号、多账号切换与登录续期
- **独立实例**：存档、模组与配置相互隔离，支持重命名、归档与恢复
- **模组与整合包**：内置 Modrinth 搜索，自动安装必需依赖，检查兼容更新，支持 `.mrpack` 拖入导入
- **Java 与内存**：按游戏版本自动选择 Java、按物理内存自动分配；Windows 可下载 Temurin JRE
- **下载源**：Mojang 官方、BMCLAPI、明叔镜像与自定义镜像，失败可回退官方源
- **内置更新**：从 GitHub Releases 检查、校验并替换新版本，保留旧版备份

## 下载

发布产物为 Windows x86_64 便携包，解压到可写目录后运行 `NSCL.exe`：

| 文件 | 内容 |
| --- | --- |
| `nscl-v<版本>-windows-x86_64.zip` | `NSCL.exe` 与 `nscl-cli.exe` |
| `SHA256SUMS` | 发布产物校验值 |

项目当前处于开发阶段，尚无稳定版本发布，可从源码构建。

## 快速开始

1. 「设置」选择游戏目录与下载源。Java 与内存默认自动分配，可按需手动指定。
2. 「账号」登录 Microsoft 账号，或填写玩家名创建离线账号。
3. 「下载 → Minecraft」选择游戏版本与加载器，点击「开始安装」。
4. 回到「开始」选择实例，点击「启动游戏」。

## 环境要求

| 项目 | 要求 |
| --- | --- |
| 操作系统 | Windows 10/11 x86_64（主要验证平台；macOS 与 Linux 尚未发布验证） |
| 网络 | 安装游戏、下载资源与 Microsoft 登录需要联网 |
| 账号 | 正版登录需要拥有 Minecraft Java Edition 授权的 Microsoft 账号 |

## 从源码构建

需要 Rust stable 的 MSVC 工具链，以及 Microsoft C++ Build Tools 的「使用 C++ 的桌面开发」组件。

```powershell
git clone https://cnb.cool/JeffreyMing/NorthStar-Minecraft-Launcher
cd NorthStar-Minecraft-Launcher
cargo build --release --locked --bin nscl
.\target\release\nscl.exe
```

提交改动前执行：

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
```

## 数据位置

用户配置保存在 `%LOCALAPPDATA%\cool.cnb.jeffreyming.nscl\`（`settings.json`、`nscl-accounts.json`、缓存与 Java 运行时）。游戏文件默认使用 `NSCL.exe` 所在目录，其中已有 `.minecraft` 时优先使用该子目录。登录刷新令牌保存在系统凭据管理器，不写入配置文件。

## 使用文档

| 文档 | 内容 |
| --- | --- |
| [原版客户端](docs/vanilla-client.md) | 安装、镜像接口、文件修复与命令行 |
| [模组加载器](docs/loaders.md) | Forge、Fabric、NeoForge 安装与 Java 要求 |
| [账号与实例](docs/accounts-and-instances.md) | 登录、凭据保存、Java 配置与实例归档 |
| [模组与整合包](docs/mods-and-modpacks.md) | 资源下载、依赖解析与整合包格式 |
| [启动器更新](docs/launcher-updates.md) | 发布流程、校验与回退 |

## 支持范围

- 模组与整合包资源目前接入 Modrinth，支持 `.mrpack` 及同格式 ZIP；CurseForge、Quilt 尚未接入。
- 兼容性检查依据资源发布信息、依赖与整合包清单，不保证任意第三方模组组合均能运行。
- 离线账号无法进入要求正版认证的服务器；正版验证失败不会自动切换为离线账号。
- Java 自动安装与重启更新目前仅支持 Windows。

## 反馈

欢迎在 [GitHub Issues](https://github.com/JeffreyMing2004/NorthStar-Minecraft-Launcher/issues) 反馈问题。提交时请附启动器版本、操作系统、Minecraft 与加载器版本、复现步骤及已脱敏日志，不要提交令牌或浏览器授权链接。

## 声明

NSCL 是独立的非官方项目，与 Mojang Studios 或 Microsoft 无隶属或背书关系。Minecraft 及相关名称、商标归其各自权利人所有。

本仓库尚未指定开源许可证，也未提供 `LICENSE` 文件。
