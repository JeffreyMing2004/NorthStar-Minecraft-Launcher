# 账号、Java 与实例管理

## Microsoft 正版登录

1. 在「设置」确认 Microsoft 应用 Client ID，默认使用 NSCL 的 `0c342b6c-38ee-414d-8f83-8df917817e2b`。
2. 在「账号」点击「登录 Microsoft 账号」，点击官方授权链接并输入启动器显示的设备代码。密码、多因素认证和授权确认由用户在 Microsoft 官方网页完成。
3. 授权后，启动器依次验证 Xbox Live、XSTS、Minecraft Java 授权和玩家档案，添加账号并选择正版身份。
4. 已添加的账号可以切换或移除；启动时检查身份，过期时自动续期。失败会提示重新登录，不会自动使用离线身份。

应用注册须支持个人 Microsoft 账号，并允许公共客户端设备代码流程；另外需要 Minecraft Java API 的访问许可。Client ID 是公开应用标识，不是 Client Secret，本应用不使用或保存 Client Secret。应用权限未获批时，Minecraft 服务可能返回 HTTP 403，需由应用所有者处理。Xbox 档案、家庭授权或首次角色设置问题会显示对应提示。

认证只连接 Microsoft、Xbox 和 Minecraft 官方服务，不经过客户端下载镜像。遵守设备代码轮询间隔、`slow_down`、有效期与取消操作；网络请求具有超时限制。优先使用当前 `/launcher/login` 和 `/entitlements/license` 接口；仅在接口返回 404/405 时兼容旧接口，认证拒绝不会触发接口回退。

Windows 使用凭据管理器，macOS 使用 Keychain，保存 Microsoft 刷新令牌；Minecraft access token 仅保存在内存并传给游戏。系统凭据库不可用时，桌面登录会明确提示仅当前会话有效；Linux 暂时只提供会话登录。移除账号会删除本机保存的刷新令牌，但不等同于撤销 Microsoft 网站上的应用授权。移除最后一个账号后，用户仍需明确选择离线测试或重新登录。

`nscl-accounts.json` 与设置文件相邻，仅包含玩家名、UUID、XUID、公开 Client ID 和当前账号选择，不包含令牌。诊断输出隐藏启动令牌，认证错误不回显原始响应。不要向他人分享 Java 进程完整命令行，游戏需要通过参数接收 access token。

协议依据：[Microsoft 设备代码授权文档](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-device-code)、[Xbox 身份认证概述](https://learn.microsoft.com/en-us/gaming/gdk/docs/services/fundamentals/s2s-auth-calls/service-authentication/live-xbox-live-authentication)、[Prism Launcher 当前认证步骤](https://github.com/PrismLauncher/PrismLauncher/tree/develop/launcher/minecraft/auth/steps)。实现通过本地模拟协议和 Windows 虚拟凭据读写测试；真实用户登录、Minecraft 应用许可和正版服务器连接需实际授权验证。

## Java 配置

- 「设置 → 扫描 Java」检查 `JAVA_HOME`、`PATH`、常见发行版目录及启动器数据目录下的 `java/`，列出可用版本、架构、发行版和路径。
- 可填写 `java` / `javaw` 可执行文件或 Java 安装目录，点击「检测当前路径」验证。单次检测最多等待约 6 秒，不阻塞界面。
- 全局路径留空时，按 Minecraft 元数据自动选择同主版本、同架构的 Java。手动指定不匹配的版本会提示错误，不会偷偷换用其他 Java。
- 实例可继承全局配置，或单独配置 Java 和 1024–32768 MiB 内存。实例 Java 覆盖开启且路径留空表示此实例使用自动匹配。
- 本次不包含 JRE 自动下载，需先安装所需 Java。

## 实例管理

在「开始 → 管理实例」中，选择已安装的 Minecraft 版本并填写名称，即可创建独立空白实例。同一客户端版本可创建多个实例，分别配置存档、模组、Java 和内存。管理窗口支持选择当前实例、重命名、打开目录、归档和恢复；游戏或任务运行期间锁定实例修改。

首次安装和旧版安装会保留原来的 `instances/<version>/`，因此已有存档不用迁移。新实例采用随机 ID；显示名称可包含中文，修改名称不会移动目录。实例配置保存在该实例目录的 `nscl-instance.json`。最近使用的实例选择会保存在本地设置中。

归档需在界面确认，整个实例目录会移入 `archived-instances/`，不会递归删除存档，也不会移除共享的版本、依赖或资源。展开「已归档实例」即可恢复；目录冲突会返回错误，避免覆盖现有数据。实例管理拒绝目录穿越、符号链接和 Windows 目录联接。归档恢复不是异地备份，重要存档仍应另行备份。
