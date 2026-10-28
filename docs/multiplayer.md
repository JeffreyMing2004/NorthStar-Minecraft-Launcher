# frpc / 88frp 联机与 30 账号白名单

启动器的「联机」页由**房主电脑运行 frpc**，自动测速选择节点、分配一条空闲隧道并分享 `frpc:…` 房间码。好友仍通过启动器授权加入，使用启动器提供的本地游戏地址。支持 Microsoft 正版账号和离线账号。全服务最多登记 **30 个固定账号**，不是每房间 30 人，也不是前 30 人自动注册。

## 架构和权限

```text
授权：房主/好友启动器 ─ HTTPS ─ nscl-relay（30 账号、隧道租约、短期加入授权）

游戏：好友 Minecraft → 好友启动器本地入口 → 节点域名:分配端口
      → 88frp 节点 → 房主 frpc → 房主启动器授权入口 → 房主游戏局域网端口
```

游戏数据直接通过 frpc 转发，授权服务只处理账号、节点、隧道分配和心跳，不再逐块中继新房间的游戏数据。旧 HTTPS 分块中继 API 保留以兼容旧房间。

**公共域名端口连接的是启动器授权入口，不能直接用原版 Minecraft 连接。** 每个连接使用新随机挑战，好友用自己的凭证向 HTTPS 服务申请短期 HMAC-SHA256 授权；房主检查房间、挑战、账号、有效期及签名，再确认授权仍有效，最后转到游戏端口。旧授权不能重放到新挑战。不要自行将游戏端口替换为直接公网映射，否则会绕过此加入校验。

房主必须在本地读取被分配节点的 frpc 认证信息；它仅在管理员私有目录及房主运行期间的临时配置中使用，不公开给好友，不写入设置、日志或整合包。不能将此机制当作防止已授权房主修改程序、提取提供商凭据或另开映射的措施。

管理员离线登记账号并发放独立的随机 64 位联机凭证。正版账号绑定 Minecraft UUID；离线账号绑定玩家名生成的离线 UUID。凭证只保存 SHA-256 摘要，服务器依据凭证确定身份，不接受客户端自行声明的用户名作为授权依据。此凭证是独立的邀请授权，不会收集 Microsoft 密码或 Minecraft 登录令牌，也不等同于再次验证 Microsoft 登录；持有凭证的人具有对应账号的联机权限，因此不能公开分享。

白名单上限在网关配置加载和管理员登记时都强制检查。第 31 个账号会被拒绝，文件手动超限或损坏也会拒绝提供服务。没有公开注册、邀请发放或增加名额的 HTTP 接口。白名单每次请求都会重新读取；撤销账号后，原凭证失效，其创建的房间或加入的数据连接也会关闭。客户端离线或崩溃时，房间和连接在约一分钟后失效。

网关不修改游戏的正版认证、白名单或模组要求。原版 LAN 世界能否接受离线玩家取决于游戏或服务端自身的认证设置；拥有启动器联机凭证不保证能进入要求正版认证的游戏世界。

## 构建和登记账号

### 节点目录与随机测速

导入器识别北京 `frp-bj.northstarmc.cn`、广州 `frp-gz.northstarmc.cn`、杭州 `frp-hz.northstarmc.cn`、上海 `frp-sh.northstarmc.cn`，保留平台的节点 IP、控制端口、`user`、代理名称、远程端口、心跳和加密/压缩设置。

连续多节点导出会重复 `serverAddr`，不能当作一份普通 TOML 直接启动。管理员先导入私有 JSON 目录，客户端仅获取不含凭据的节点元数据，申请房间后房主才取得一条被分配隧道的配置。

创建房间时随机打乱节点顺序，并发对控制端口各探测 3 次，使用成功样本的中位 TCP 延迟，跳过不可达节点；最快延迟附近 10 ms 内的节点随机优先。这是连接延迟/可达性测试，不是 Mbps 吞吐测速。授权服务按该排序原子分配空闲隧道，防止多个房主抢占；frpc 启动失败或端口被占用时释放租约并尝试其他条目，最多 8 次。

运行配置使用 FRP 原生 JSON，仅包含当前代理。认证信息、代理名和远程端口不变，`localIP` 固定回环地址，`localPort` 改为启动器的动态授权入口，再由它转发至用户填写的游戏端口。frpc 原始输出不会显示或记录，只解析成功、占用等固定状态。取消、失败或停止会结束进程并清理临时配置；Windows Job Object 会在启动器异常退出时结束 frpc。

仅使用调试构建：

```powershell
cargo build --bin nscl-relay
New-Item -ItemType Directory -Force relay-data
target/debug/nscl-relay.exe init relay-data/members.json
target/debug/nscl-relay.exe add-offline relay-data/members.json Alice
target/debug/nscl-relay.exe add-microsoft relay-data/members.json 12345678-1234-1234-1234-123456789abc OnlinePlayer
target/debug/nscl-relay.exe list relay-data/members.json
```

`add-*` 成功后只显示一次联机凭证，私下交付给对应玩家。示例中的正版 UUID 必须换成玩家真实的 Minecraft UUID，可在启动器账号信息中核对。离线玩家修改名称后会变成另一个账号，需重新登记。丢失凭证时先撤销原账号再重新登记；用户自己不能获取或重置凭证。

撤销权限：

```powershell
target/debug/nscl-relay.exe remove relay-data/members.json UUID
```

撤销后释放一个白名单名额。管理员操作通过本地文件锁串行执行；异常终止遗留 `.lock` 文件时，确认没有其他管理员命令运行后再手动移除。白名单文件应只允许网关运行用户和管理员访问，勿放在 Web 静态文件目录。

## 发布 HTTPS 与 88frp

1. 将平台导出的原始多节点配置保存到 `relay-data/frpc-source.toml`，运行下面的导入命令；已有私有 JSON 时可以直接使用。
2. 使用节点目录启动授权服务。Linux 使用无 `.exe` 的程序。服务仍只允许监听回环地址。
3. 用 [nginx HTTPS 示例](relay/nginx.example.conf) 或同等代理配置有效域名证书，向玩家提供 HTTPS **授权服务地址**。可用独立管理隧道发布该入口，但不能占用房间池中同一条隧道。[旧网关发布示例](relay/frpc.88frp.example.toml) 只用于此管理入口，与房主电脑运行的 frpc 分开。
4. 将四个房间域名解析到对应 88frp 节点。房主启动器管理游戏隧道的 frpc 进程；授权服务和 HTTPS 入口由系统服务管理器维护。不记录 Authorization 头，不向外发布私有配置和白名单。

```powershell
target/debug/nscl-relay.exe import-frpc relay-data/frpc-source.toml relay-data/frpc-nodes.json
target/debug/nscl-relay.exe probe-frpc relay-data/frpc-nodes.json
target/debug/nscl-relay.exe serve relay-data/members.json 127.0.0.1:17630 relay-data/frpc-nodes.json
```

`probe-frpc` 仅显示节点区域和 TCP 延迟。实际源配置、节点 JSON 与白名单保存在 Git 忽略的 `relay-data/`，读取权限应只授予管理员和服务账号。当前 4×7 条隧道最多提供 28 个同时创建的房间，与 30 个账号名额是不同限制。

仍需要持续运行的 HTTPS 授权服务、真实账号名单以及房主本地 frpc 程序。实际节点、端口、流量和使用条件以 [88frp 平台](https://88frp.com/) 和[登录后的配置指南](https://88frp.com/client/help-support/FRPC%20%E9%85%8D%E7%BD%AE%E6%8C%87%E5%8D%97)为准。不会自动注册、购买套餐、创建/修改平台隧道或下载尚未提供地址的 frpc 程序。

## 玩家操作

1. 在启动器账号页选择管理员登记的正版或离线账号。
2. 打开「联机」，填写管理员提供的 HTTPS 服务地址和该账号的联机凭证。可保存服务地址；凭证只在本次启动器运行期间保留，不写入设置文件、日志或整合包。
3. 房主展开「房主 frpc 程序设置」，选择本地官方或平台配套 frpc 程序，保存联机设置。进入游戏并「对局域网开放」，填写游戏显示的端口，点击「创建联机房间」。测速和 frpc 代理启动成功后会显示节点、TCP 延迟、隧道域名端口与 `frpc:…` 房间码。
4. 好友使用同一授权地址、自己的凭证和房间码点击「加入联机房间」，不需要安装 frpc，也不会取得节点配置。复制显示的 `127.0.0.1:端口`，在 Minecraft 多人游戏中直接连接。
5. 双方保持启动器开启、房主世界开放，游戏版本和模组应兼容。切换账号、主动断开或退出会结束联机，启动过程中可以取消。设置中仅保存授权服务地址和 frpc 程序路径，联机凭证只保留在本次运行中。

## 验证与边界

自动测试覆盖多节点导入、单隧道配置保真、随机 TCP 测速、不可达节点、独占租约和释放、伪造签名拦截、挑战绑定、撤销权限断连、frpc 成功/占用/取消与临时文件清理，以及受控 frpc 测试替身进程下的真实 TCP 游戏数据往返。旧中继及 30 账号边界测试继续保留；界面测试覆盖最小窗口和切换账号清除凭证。

新房间直接使用 TCP 数据通道；旧 HTTPS 房间继续使用最多 64 KiB 分块和每方向 512 KiB 缓冲。每账号最多创建一个房间，授权服务重启会结束全部房间。不提供跨版本模组同步或游戏中无缝节点切换。

本地测试替身不等于真实 88frp 公网链路验收。还需要部署授权服务、填写真实名单和指定房主 frpc 程序；TCP 节点探测也不代表 30 人同时游戏的吞吐或延迟保证。

参考：[88frp 官网](https://88frp.com/)、[FRP 配置说明](https://gofrp.org/en/docs/features/common/configure/)、[HMAC-SHA256](https://docs.rs/hmac/0.12.1/hmac/)、[nginx 反向代理](https://nginx.org/en/docs/http/ngx_http_proxy_module.html)。
