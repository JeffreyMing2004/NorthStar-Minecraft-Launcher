# NSCL 测试版使用说明

这是 Windows 10 / 11 x86_64 的便携调试测试版，尚未完成所有真实账号、网络和游戏组合的人工验证。

1. 将 ZIP **完整解压**到用户可写的独立文件夹，再运行 `NSCL.exe`。不要直接在压缩包中运行。
2. 首次使用建议在「设置」选择新的游戏目录，避免与日常游玩的实例混用。
3. 在「账号」添加 Microsoft、外置认证或离线账号。离线账号不能加入要求正版认证的服务器。
4. 在「下载 → Minecraft」安装游戏和可选加载器，然后在「开始」选择实例启动。缺少 Java 时按启动前检查提示安装。

发布页也提供同一构建的独立 EXE，可放入新的可写文件夹后直接运行；ZIP 另外包含本地使用文档和许可证。两种文件均有 SHA-256 校验值。

调试版的 `nscl-settings.json`、账号索引和缓存位于程序旁；刷新令牌保存在系统凭据库。默认游戏目录也是程序所在目录，可在设置中更改。升级时退出启动器，只替换程序；保留原有设置和游戏目录。

此版未配置 CurseForge API Key，请使用 Modrinth 或本地文件。CurseForge 整合包仍需 API 元数据，暂不能完整安装。联机依赖管理服务、可用隧道和实际网络条件。离线皮肤是启动器预览，不保证游戏内显示。

「更多」可检查测试版与正式版更新；版本按语义化版本比较，不自动降级。正式版客户端只跟随正式通道。

窗口打不开时，保留程序目录下的 `logs/nscl-startup.log`；可尝试通过终端运行 `NSCL.exe --renderer software`（独立 EXE 请替换为实际文件名）。软件渲染速度较慢，主要用于兼容性排查。

游戏故障可导出脱敏诊断包。到 [GitHub Issues](https://github.com/JeffreyMing2004/NorthStar-Minecraft-Launcher/issues/new/choose) 反馈时，请附启动器版本、Windows 版本、游戏和加载器版本、操作步骤及诊断包。分享原始日志前请自行移除账号、令牌和个人路径。

更多说明：[使用指南](docs/user-guide.md)、[故障排查](docs/troubleshooting.md)、[实例备份](docs/instance-backups.md)、[更新与回退](docs/launcher-updates.md)、[联机](docs/multiplayer.md)。

源码与对应标签：[GitHub](https://github.com/JeffreyMing2004/NorthStar-Minecraft-Launcher) / [CNB](https://cnb.cool/JeffreyMing/NorthStar-Minecraft-Launcher)。本项目使用 [GPL-3.0-only](LICENSE)，与 Mojang Studios 或 Microsoft 无隶属或背书关系。
