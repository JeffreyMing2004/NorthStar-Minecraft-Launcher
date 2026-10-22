# 账号、Java 与实例管理

## Microsoft 正版登录

应用 Client ID 和本机登录回调地址由启动器在后台管理，设置页不显示这两项，无需手动配置。

1. 在「账号」点击「登录 Microsoft 账号」，启动器会打开系统浏览器。密码、多因素认证和授权确认由用户在 Microsoft 官方网页完成；若未弹出浏览器，可点击「重新打开浏览器」或「复制登录链接」，将链接粘贴到浏览器地址栏。完成授权前请保持启动器打开。
2. 授权后，启动器依次验证 Xbox Live、XSTS、Minecraft Java 授权和玩家档案，添加账号并选择正版身份。
3. 已添加的账号可以切换或移除；启动时检查身份，过期时自动续期。失败会提示重新登录，不会自动使用离线身份。

NSCL 内置应用 Client ID `0c342b6c-38ee-414d-8f83-8df917817e2b` 和默认回调 `http://localhost:29111/auth-response`，已有后台配置继续保留。应用注册须支持个人 Microsoft 账号，本机回调需要登记在「移动和桌面应用程序」平台。NSCL 使用授权码 + PKCE（S256）与随机 state 校验，不使用或保存 Client Secret。仅配置重定向 URI 并不会启用设备代码流程，因此默认按钮使用浏览器登录；需要设备代码时，在「其他登录方式」中选择，并确保应用允许该流程。另外仍需 Minecraft Java API 的访问许可；权限未获批时，Minecraft 服务可能返回 HTTP 403，需由应用所有者处理。Xbox 档案、家庭授权或首次角色设置问题会显示对应提示。

默认回调端口被占用时，启动器会尝试已经为 NSCL 登记的 `29112–29115`，路径均为 `/auth-response`。如后台配置使用其他回调地址，仅使用该地址的端口与路径。监听器只绑定本机回环地址，检查回调路径、Host 与 state，忽略无关请求，在收到授权码、取消或十分钟超时后关闭；兑换令牌时使用完全相同的回调 URI 与 PKCE verifier。

Windows 使用原生 ShellExecuteEx 接口打开默认浏览器，检查调用结果，失败后尝试直接调用已注册的浏览器；不通过 PowerShell 转交登录链接。打开浏览器在后台执行，失败会在账号页显示错误，同时保留回调监听与复制链接入口。如系统没有默认浏览器，请先在 Windows「设置 → 应用 → 默认应用」中设置 HTTP/HTTPS 浏览器，然后重试。设备代码页面也支持打开浏览器与复制链接。

如果 Edge 提示「现有实例正在以提升的权限运行」，说明浏览器实例与新请求的权限不一致。先保存现有网页中未保存的内容，再在 Edge 提示中选择以普通权限重启，或手动关闭 Edge 后以普通权限重新打开；启动器也应以普通权限运行。随后点击「重新打开浏览器」，或复制登录链接到已打开的浏览器。启动器不会自动关闭浏览器、重启现有标签页或请求管理员权限。系统接受打开请求不代表浏览器已显示网页，因此账号页保留重试与复制入口。

排查应用配置时，错误提示会保留 AADSTS 数字编号并给出对应说明，不回显含令牌等信息的原始响应。例如 `AADSTS50011` 表示回调不匹配，`AADSTS700016` 可用于排查应用标识或账号范围，`AADSTS7000218` 提示检查回调的平台类型。不要为桌面启动器添加或提供 Client Secret。

认证只连接 Microsoft、Xbox 和 Minecraft 官方服务，不经过客户端下载镜像。遵守设备代码轮询间隔、`slow_down`、有效期与取消操作；网络请求具有超时限制。优先使用当前 `/launcher/login` 和 `/entitlements/license` 接口；仅在接口返回 404/405 时兼容旧接口，认证拒绝不会触发接口回退。

Windows 使用凭据管理器，macOS 使用 Keychain，保存 Microsoft 刷新令牌；Minecraft access token 仅保存在内存并传给游戏。系统凭据库不可用时，桌面登录会明确提示仅当前会话有效；Linux 暂时只提供会话登录。移除账号会删除本机保存的刷新令牌，但不等同于撤销 Microsoft 网站上的应用授权。移除最后一个账号后，用户仍需明确选择离线测试或重新登录。

`nscl-accounts.json` 与设置文件相邻，仅包含玩家名、UUID、XUID、公开 Client ID 和当前账号选择，不包含令牌。诊断输出隐藏启动令牌，认证错误不回显原始响应。不要向他人分享 Java 进程完整命令行，游戏需要通过参数接收 access token。

协议依据：[Microsoft 授权码与 PKCE 文档](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-auth-code-flow)、[回调 URI 配置说明](https://learn.microsoft.com/en-us/entra/identity-platform/reply-url)、[设备代码授权文档](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-device-code)、[Xbox 身份认证概述](https://learn.microsoft.com/en-us/gaming/gdk/docs/services/fundamentals/s2s-auth-calls/service-authentication/live-xbox-live-authentication)、[Prism Launcher 当前认证步骤](https://github.com/PrismLauncher/PrismLauncher/tree/develop/launcher/minecraft/auth/steps)。测试使用本地模拟认证服务与虚拟系统凭据；真实用户登录、Minecraft 应用许可和正版服务器连接需实际授权验证。

## 皮肤管理与 3D 预览

首页和账号页使用 Rust 原生绘制带纹理的 3D 角色，支持拖动旋转与双击复位、标准手臂与纤细手臂、帽子及衣物外层。登录或切换账号后自动读取当前正版皮肤；读取失败时可显示本地缓存，首次未读取到皮肤时显示启动器默认预览模型。

在账号页填写或拖入 PNG 文件，选择模型后点击「预览本地皮肤」。支持 64×64 与传统 64×32 PNG，最大 1 MiB；传统皮肤仅支持标准手臂，预览时自动补齐镜像肢体。点击「上传到当前正版账号」才会修改在线皮肤；上传前重新验证当前账号身份。可点击「刷新账号皮肤」恢复在线显示。游戏中的新皮肤可能需要重新进入服务器。

皮肤通过 Minecraft 官方角色服务上传，纹理从 `textures.minecraft.net` 获取，下载纹理时不携带账号令牌。缓存位于设置文件旁的 `cache/skins/`，仅包含皮肤图片与模型类型。模拟服务测试验证上传表单、身份匹配、图片格式及纹理请求不泄漏令牌；真实账号上传需用户在界面主动操作。

接口参考：[Minecraft 用户服务实现](https://github.com/Voxelum/minecraft-launcher-core-node/blob/master/packages/user/mojang.ts)。

## Java 配置

- 「设置 → 扫描 Java」检查 `JAVA_HOME`、`PATH`、常见发行版目录及启动器数据目录下的 `java/`，列出可用版本、架构、发行版和路径。
- 可填写 `java` / `javaw` 可执行文件或 Java 安装目录，点击「检测当前路径」验证。单次检测最多等待约 6 秒，不阻塞界面。
- 全局路径留空时，按 Minecraft 元数据自动选择同主版本、同架构的 Java。手动指定不匹配的版本会提示错误，不会偷偷换用其他 Java。
- 实例可继承全局配置，或单独配置 Java 和 1024–32768 MiB 内存。实例 Java 覆盖开启且路径留空表示此实例使用自动匹配。
- Windows 可在设置中选择 Java 8、11、17、21、25，点击「下载并安装」自动获取 Eclipse Temurin JRE；「按当前实例选择」可选中已安装实例要求的主版本。运行时下载完成并验证 SHA-256 后解压，执行版本和架构检测，全部通过才发布到设置文件旁的 `java/temurin-<主版本>-<架构>-<校验前缀>/`，自动扫描与匹配会识别它。其他系统可继续手动指定 Java。
- 下载时显示字节进度并支持取消，下载包在 `java/.downloads/` 缓存并复核；未通过校验、解压失败或取消不会发布半完成运行时。不会修改系统 Java、注册表或 `JAVA_HOME`，手动指定的 Java 仍优先使用。
- Java 来自 [Adoptium 官方 API](https://github.com/adoptium/api.adoptium.net/blob/main/docs/cookbook.adoc) 和 Adoptium GitHub 发布资产，不经过 Minecraft 下载镜像。缺少 Java 时可先在设置中安装，再重试游戏或加载器安装。

## 实例管理

在「开始 → 管理实例」中，选择已安装的 Minecraft 版本并填写名称，即可创建独立空白实例。同一客户端版本可创建多个实例，分别配置存档、模组、Java 和内存。管理窗口支持选择当前实例、重命名、打开目录、归档和恢复；游戏或任务运行期间锁定实例修改。

首次安装和旧版安装会保留原来的 `instances/<version>/`，因此已有存档不用迁移。新实例采用随机 ID；显示名称可包含中文，修改名称不会移动目录。实例配置保存在该实例目录的 `nscl-instance.json`。最近使用的实例选择会保存在本地设置中。

归档需在界面确认，整个实例目录会移入 `archived-instances/`，不会递归删除存档，也不会移除共享的版本、依赖或资源。展开「已归档实例」即可恢复；目录冲突会返回错误，避免覆盖现有数据。实例管理拒绝目录穿越、符号链接和 Windows 目录联接。归档恢复不是异地备份，重要存档仍应另行备份。
