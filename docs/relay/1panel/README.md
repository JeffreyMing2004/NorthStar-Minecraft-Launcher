# 1Panel 联机验证服务部署

这份服务部署在你自己的服务器上，负责节点信息、最多 28 个房间的隧道分配和心跳管理。开房和加入均无需账号登记或开房码。MC 游戏数据由房主电脑上的 frpc 通过 TCP 原样转发；旧连接票据 API 保留兼容。

启动器内置 `https://auth.minguncle.cn`，连接前检查可用性，失败时尝试备用地址 `https://auth.northstarmc.cn`。两个域名应反向代理到同一份后端实例，确保共享隧道占用状态。北京、广州、杭州、上海的 FRP 域名继续解析到 88frp 节点，由启动器自动选择。

**只能使用本机 25565 端口开房**，请将 Minecraft 的局域网端口或服务端端口设置为 25565，直接点击创建房间。最多 28 位房主同时开房，每个房间独占一条隧道；关闭或超时后释放名额。加入者人数由 Minecraft 控制。

## 1. 准备部署包

维护者在项目目录运行：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/Package-Relay.ps1 -IncludePrivateCatalog
```

输出在 `target/relay-packages/`。`-private-` 包包含现有的私有映射池，只上传到自己的服务器；Git 不会包含该包或真实配置。省略此参数可生成公开模板包，需要自行补充 `relay-data/frpc-nodes.json`。新服务只需要节点池，不使用 `members.json` 或凭证表。

包的目录结构：

```text
docker-compose.yml       Docker 编排
Dockerfile               在服务器上构建 Linux 调试版
.dockerignore            构建镜像时排除私有配置
configure-apt.sh         腾讯云 APT 换源
mirrors.md               Docker / APT / Cargo 镜像说明
openresty-location.conf  可选反向代理片段
relay-data/
  frpc-nodes.json         私有节点和隧道池
source/                  与启动器同一版本的服务源码、锁文件和许可证
```

## 2. 在 1Panel 构建并启动

使用 Linux 服务器上的 1Panel，安装并启动 Docker、OpenResty。通过「文件」上传并解压包到 `/opt/nscl-relay`，确认 `docker-compose.yml` 位于这个目录下。

在 1Panel 终端执行：

```sh
cd /opt/nscl-relay
chmod 700 relay-data
chmod 600 relay-data/*.json
chown -R 10001:10001 relay-data
docker compose config --quiet
docker compose build
docker compose up -d
docker compose ps
curl --fail http://127.0.0.1:17630/v1/frpc/health
```

腾讯云服务器建议先按 [镜像配置说明](mirrors.md) 设置 Docker 内网加速。容器内默认使用腾讯云 APT 和南京大学 Cargo 源，并支持切回官方源。Dockerfile 仅执行调试构建，使用 `--no-default-features` 跳过 GUI 依赖。容器以 UID/GID 10001 读取私有节点池，目录和文件需要授予该用户读取权限。

健康检查应返回类似：

```json
{"service":"northstar-multiplayer","protocol":1,"max_hosts":28,"nodes":4,"tunnels":28,"host_codes":false,"anonymous_hosts":true,"game_port":25565}
```

也可在「容器 → 编排」中通过路径选择该 `docker-compose.yml` 管理服务；菜单名称随 1Panel 版本略有差异。模板依赖同目录的 Dockerfile 和源码，不能只把 YAML 文本单独粘贴到另一个目录。若面板未自动构建镜像，先执行上述 `docker compose build`。

服务使用 Linux host 网络，仅监听宿主机 `127.0.0.1:17630`，不对公网开放该端口。保持单实例运行：隧道租约在服务内存中管理，多实例不共享占用状态。重启服务会结束现有房间，玩家重新开房即可。

## 3. 配置验证域名和 HTTPS

将 `auth.minguncle.cn` 和 `auth.northstarmc.cn` 解析到你的服务器。1Panel「网站」创建反向代理网站：

- 绑定你的验证域名。
- 代理地址填写 `http://127.0.0.1:17630`，代理整个 `/` 路径。
- 申请或导入该域名证书，启用 HTTPS。
- 关闭代理缓存，保留 Authorization 请求头，不设置网站登录密码验证。

本配置适用于使用宿主机网络的 OpenResty。若你更改过 OpenResty 的容器网络，先确认它能访问宿主机回环地址，再配置代理。需要手动配置时，可参考包内 `openresty-location.conf`，替换站点现有的 `location /`，避免重复添加。

验证外网地址：

```sh
curl --fail https://auth.minguncle.cn/v1/frpc/health
curl --fail https://auth.northstarmc.cn/v1/frpc/health
```

启动器只填写 HTTPS 根地址，不要附加 `/v1` 或健康检查路径。公网使用 HTTPS 传输房间管理信息和隧道配置，服务器内部回环请求使用 HTTP；游戏连接使用 frpc TCP。健康响应应有 `anonymous_hosts: true` 和 `game_port: 25565`，缺少时请更新后端镜像。

## 4. 直接开房

房主打开启动器的「联机」，启动器自动使用内置主备管理服务并检测 frpc，左下角显示状态。将游戏端口设为 25565 后，点击「创建联机房间」即可。无需填写服务地址、开房码或登记账号。满 28 个房间时，等待其他房间关闭后重试。自建发行版的管理服务地址在 `src/multiplayer/frpc/mod.rs` 中修改后重新构建。

创建成功后会显示 Minecraft 直连地址和房间码。好友可直接在 Minecraft 中连接该域名端口，或用房间码通过启动器加入，不需要 frpc。房主与使用房间码的好友应更新启动器并重新创建房间，以避免旧版自定义握手与 Minecraft 协议冲突；现有匿名后端无需为此重新部署。游戏自身的正版认证、白名单、模组要求和最大玩家数继续生效。

启动器自动维护房间心跳和关闭权限，相关管理信息只在内存中保存，不需要管理员发放。房主主动断开会释放隧道；失联租约约一分钟后失效。服务端在同一把锁中检查房间数量并分配隧道，避免同时创建超过 28 个房间。

## 5. 维护

```sh
cd /opt/nscl-relay
docker compose logs --tail 100 relay
docker compose restart relay
docker compose down
```

私有配置保留在 `relay-data/`，备份这个目录。更新包时保留节点池，更新源码和 Dockerfile 后执行 `docker compose up -d --build`。新镜像启动命令为 `serve-frpc /data/frpc-nodes.json 127.0.0.1:17630`，无需名单文件，旧凭证文件不会影响新服务。

本机健康检查失败时先看容器日志，核对目录权限、节点 JSON 及 17630 端口是否被占用。外网返回 502 时核对 OpenResty 代理地址；HTML 页面或 404 通常表示域名代理到了其他网站或路径错误。

参考：[1Panel 编排](https://docs.fit2cloud.com/1panel/v1/user-manual/containers/compose/)、[创建反向代理网站](https://docs.fit2cloud.com/1panel/user_manual/websites/website-create/)、[网站 HTTPS 配置](https://docs.fit2cloud.com/1panel/user_manual/websites/website-config-basic/)、[官方 Rust 镜像](https://github.com/docker-library/official-images/blob/master/library/rust)。
