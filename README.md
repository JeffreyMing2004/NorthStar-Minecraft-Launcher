<div align="center">
  <img src="assets/nscl-logo.svg" width="112" height="112" alt="NSCL 四角星标志" />
  <h1>NorthStar Minecraft Launcher</h1>
  <p>Minecraft Java Edition 桌面启动器</p>
  <p>
    <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-native-CE6F3F?style=flat-square&amp;logo=rust" alt="使用 Rust 构建" /></a>
    <img src="https://img.shields.io/badge/Windows-10%20%2F%2011-7DA9D8?style=flat-square" alt="Windows 10 / 11" />
    <a href="LICENSE"><img src="https://img.shields.io/badge/License-GPL--3.0--only-78BFA0?style=flat-square" alt="GPL-3.0-only" /></a>
  </p>
  <p>
    <a href="#功能特性">功能特性</a> ·
    <a href="#快速开始">快速开始</a> ·
    <a href="https://github.com/JeffreyMing2004/NorthStar-Minecraft-Launcher/issues">问题反馈</a> ·
    <a href="https://cnb.cool/JeffreyMing/NorthStar-Minecraft-Launcher">CNB 镜像</a>
  </p>
</div>

---

NorthStar Minecraft Launcher（NSCL）是使用 Rust 构建的 Minecraft Java Edition 启动器，面向 Windows 10 / 11 x86_64，提供游戏安装、账号管理、独立实例、模组与整合包管理及联机功能。

## 功能特性

- **游戏安装**：安装、校验和修复 Minecraft，支持 Forge、Fabric 与 NeoForge。
- **账号管理**：Microsoft 登录、离线账号、多账号切换，以及皮肤上传、3D 预览和正版账号披风选择。
- **实例管理**：独立或共享游戏目录，复用已有 `.minecraft`，支持实例配置、归档与恢复。
- **内容管理**：搜索 Modrinth 模组，处理必需依赖、兼容性检查和更新；管理本地模组、资源包与光影。
- **整合包**：导入与导出 Modrinth `.mrpack` 整合包，按包要求准备游戏及加载器。
- **运行环境**：自动匹配 Java 和内存配置，支持手动调整；Windows 可下载所需 Java 运行时。
- **联机**：自动下载 frpc、测速并分配隧道，无需开房码，最多同时开放 28 个房间。
- **下载与更新**：下载源自动切换，文件校验与缓存复用，支持启动器更新检查。

## 界面预览

<p align="center">
  <img src="docs/images/home.png" width="1100" alt="NSCL 开始页：游戏实例选择与角色区域" />
  <br /><sub>开始页 · 游戏实例与角色入口</sub>
</p>

<p align="center">
  <img src="docs/images/downloads.png" width="1100" alt="NSCL 下载页：自动下载源、版本列表与加载器安装入口" />
  <br /><sub>下载页 · 自动切换来源，选择游戏版本与加载器</sub>
</p>

## 快速开始

当前通过源码构建运行。可用发布包以 [GitHub Releases](https://github.com/JeffreyMing2004/NorthStar-Minecraft-Launcher/releases) 为准。

1. 在「设置」选择游戏目录。默认使用启动器所在目录；若目录中有 `.minecraft`，会优先使用它。
2. 在「账号」登录 Microsoft 账号，或创建离线账号。正版登录需要拥有 Minecraft Java Edition 授权。
3. 打开「下载 → Minecraft」，选择版本与加载器，确认后开始安装。下载源默认自动选择。
4. 回到「开始」选择实例，点击「启动游戏」。

已有游戏可以直接添加其 `.minecraft` 目录；本地 `.mrpack` 整合包可拖入启动器导入。

## 参考与致谢

感谢 [HMCL](https://github.com/HMCL-dev/HMCL)、[PCL](https://github.com/Meloong-Git/PCL) 和 [Axolotl Launcher](https://github.com/Axolotl-Launcher/Axolotl) 提供的设计参考，以及 [eframe / egui](https://github.com/emilk/egui)、[Modrinth](https://modrinth.com/)、[BMCLAPI](https://bmclapi2.bangbang93.com/) 和 [minguncle 镜像](https://mirror.minguncle.cn/) 等项目与服务。

## 反馈与参与

欢迎提交 [GitHub Issue](https://github.com/JeffreyMing2004/NorthStar-Minecraft-Launcher/issues) 或 Pull Request。反馈问题时，请说明启动器版本、操作系统、游戏与加载器版本、复现步骤，并附已脱敏的错误信息或日志。

源代码托管于 [GitHub](https://github.com/JeffreyMing2004/NorthStar-Minecraft-Launcher) 与 [CNB](https://cnb.cool/JeffreyMing/NorthStar-Minecraft-Launcher)。

## 许可与声明

本项目采用 [GNU GPL v3.0（仅限第 3 版）](LICENSE)，即 `GPL-3.0-only`。修改和再分发时请遵守许可证，并按其要求提供相应源代码。

NSCL 是独立的非官方项目，与 Mojang Studios 或 Microsoft 无隶属或背书关系。Minecraft 及相关名称、商标与素材归其各自权利人所有，Minecraft 相关素材不受本项目许可证覆盖。
