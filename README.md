# NorthStar Minecraft Launcher

**NSCL** 是一款使用 Rust 与 eframe/egui 构建的 Minecraft Java Edition 桌面启动器。原生窗口、单一可执行文件、无需额外运行库。

[下载](https://github.com/JeffreyMing2004/NorthStar-Minecraft-Launcher/releases) · [问题反馈](https://github.com/JeffreyMing2004/NorthStar-Minecraft-Launcher/issues) · [CNB 镜像](https://cnb.cool/JeffreyMing/NorthStar-Minecraft-Launcher)

## 功能

- **安装与启动**：正式版、快照与历史版本，下载、校验与修复游戏文件
- **模组加载器**：Forge、Fabric、NeoForge，按所选版本自动匹配安装
- **账号**：Microsoft 浏览器授权、离线账号、多账号切换与登录续期
- **版本隔离**：可选择独立或共享游戏目录，分别管理存档、模组和配置，支持实例重命名、归档与恢复
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

## 反馈

欢迎在 [GitHub Issues](https://github.com/JeffreyMing2004/NorthStar-Minecraft-Launcher/issues) 反馈问题。提交时请附启动器版本、操作系统、Minecraft 与加载器版本、复现步骤及已脱敏日志，不要提交令牌或浏览器授权链接。

## 声明

NSCL 是独立的非官方项目，与 Mojang Studios 或 Microsoft 无隶属或背书关系。Minecraft 及相关名称、商标归其各自权利人所有。

本仓库尚未指定开源许可证，也未提供 `LICENSE` 文件。
