# 1Panel 联机验证服务部署

这份服务部署在你自己的服务器上，负责房主凭证验证、节点信息、28 个房间的隧道分配和短期加入票据。MC 游戏数据由房主电脑上的 frpc 通过 TCP 转发。

启动器的「验证服务地址」填写 `https://auth.example.com`，域名解析到你的服务器。北京、广州、杭州、上海的 FRP 域名继续解析到 88frp 节点，由启动器自动选择，无需填进这个输入框。

## 1. 准备部署包

维护者在项目目录运行：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/Package-Relay.ps1 -IncludePrivateCatalog
```

输出在 `target/relay-packages/`。`-private-` 包包含现有的私有映射池，只上传到自己的服务器；Git 不会包含该包或真实配置。省略此参数可生成公开模板包，需要自行补充 `relay-data/frpc-nodes.json`。包内的房主名单初始为空，不复制已有账号或凭证。

包的目录结构：

```text
docker-compose.yml       Docker 编排
Dockerfile               在服务器上构建 Linux 调试版
.dockerignore            构建镜像时排除私有配置
openresty-location.conf  可选反向代理片段
relay-data/
  members.json           房主名单（仅保存凭证摘要）
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

首次构建需要连接 Docker 官方镜像仓库和 crates.io。Dockerfile 仅执行调试构建，使用 `--no-default-features` 跳过桌面 GUI 依赖，服务器无需安装 Rust、PHP 或数据库。UID/GID 10001 为容器运行用户；账号管理命令也使用此用户写入名单文件。

健康检查应返回类似：

```json
{"service":"northstar-multiplayer","protocol":1,"max_hosts":28,"nodes":4,"tunnels":28}
```

也可在「容器 → 编排」中通过路径选择该 `docker-compose.yml` 管理服务；菜单名称随 1Panel 版本略有差异。模板依赖同目录的 Dockerfile 和源码，不能只把 YAML 文本单独粘贴到另一个目录。若面板未自动构建镜像，先执行上述 `docker compose build`。

服务使用 Linux host 网络，仅监听宿主机 `127.0.0.1:17630`，不对公网开放该端口。保持单实例运行：隧道租约在服务内存中管理，多实例不共享占用状态。重启服务会结束现有房间，玩家重新开房即可。

## 3. 配置验证域名和 HTTPS

将一个独立子域名（例如 `auth.example.com`）解析到你的服务器。1Panel「网站」创建反向代理网站：

- 绑定你的验证域名。
- 代理地址填写 `http://127.0.0.1:17630`，代理整个 `/` 路径。
- 申请或导入该域名证书，启用 HTTPS。
- 关闭代理缓存，保留 Authorization 请求头，不设置网站登录密码验证。

本配置适用于使用宿主机网络的 OpenResty。若你更改过 OpenResty 的容器网络，先确认它能访问宿主机回环地址，再配置代理。需要手动配置时，可参考包内 `openresty-location.conf`，替换站点现有的 `location /`，避免重复添加。

验证外网地址：

```sh
curl --fail https://auth.example.com/v1/frpc/health
```

启动器只填写 `https://auth.example.com`，不要附加 `/v1` 或健康检查路径。公网使用 HTTPS 保护房主凭证和隧道配置；服务器内部的回环请求使用 HTTP。游戏连接继续使用 frpc TCP 隧道。

## 4. 发放房主凭证

在 `/opt/nscl-relay` 下，为离线房主登记账号：

```sh
docker compose run --rm --no-deps relay add-offline /data/members.json JeffreyMing
```

为正版房主登记账号（替换为真实 Minecraft UUID 和玩家名）：

```sh
docker compose run --rm --no-deps relay add-microsoft /data/members.json 12345678-1234-1234-1234-123456789abc OnlinePlayer
```

命令会仅显示一次 64 位房主凭证，私下交给该房主。启动器选择对应正版或离线账号，填写验证服务地址和房主凭证，点击「测试验证服务」，再创建房间。离线账号改名会改变身份，需要重新登记。

仅加入的好友填写同一验证服务地址和房间码，不需要登记或填写凭证。最多同时开房 28 个，加入人数由 Minecraft 控制。

查看、撤销房主：

```sh
docker compose run --rm --no-deps relay list /data/members.json
docker compose run --rm --no-deps relay remove /data/members.json UUID
```

撤销和名单更新会在后续请求中生效，无需重启。撤销房主会关闭其房间并释放隧道。

## 5. 维护

```sh
cd /opt/nscl-relay
docker compose logs --tail 100 relay
docker compose restart relay
docker compose down
```

私有配置和名单保留在 `relay-data/`，备份这个目录。更新包时保留该目录，尤其不要用新包的空 `members.json` 覆盖现有名单；更新源码后重新执行 `docker compose build` 和 `docker compose up -d`。

本机健康检查失败时先看容器日志，核对目录权限、JSON 文件及 17630 端口是否被占用。外网返回 502 时核对 OpenResty 代理地址；HTML 页面或 404 通常表示域名代理到了其他网站或路径错误。启动器仍需选择与凭证一致的房主账号，服务不会签发匿名房主凭证。

参考：[1Panel 编排](https://docs.fit2cloud.com/1panel/v1/user-manual/containers/compose/)、[创建反向代理网站](https://docs.fit2cloud.com/1panel/user_manual/websites/website-create/)、[网站 HTTPS 配置](https://docs.fit2cloud.com/1panel/user_manual/websites/website-config-basic/)、[官方 Rust 镜像](https://github.com/docker-library/official-images/blob/master/library/rust)。
