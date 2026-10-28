# 28 条 frpc 隧道池

私有池已按平台导出的参数配置：

| 节点 | 玩家连接域名 | 隧道数 |
| --- | --- | --- |
| 北京 | frp-bj.northstarmc.cn | 7 |
| 广州 | frp-gz.northstarmc.cn | 7 |
| 杭州 | frp-hz.northstarmc.cn | 7 |
| 上海 | frp-sh.northstarmc.cn | 7 |

共 28 条 TCP 隧道。节点认证信息、代理名和远程端口保持平台原值，开启加密和压缩。房主启动器随机测速排序，验证服务原子分配一条空闲隧道，一个房主独占一条，最多同时开房 28 个。加入者不占隧道名额，人数由 Minecraft 控制。

`frpc-nodes.json` 是 **nscl-relay 的私有映射池**，由验证服务读取。房主的 frpc 配置由启动器按租约生成，只启用分配到的一条代理；无需在服务器启动这 28 条游戏代理。房主只可使用 Minecraft 的本机 25565 端口开房，启动器负责本地转发。池中的远程端口保持平台原值。

## 上传到 1Panel

1. 上传前备份服务器现有的 `relay-data/frpc-nodes.json`（如果存在）。
2. 解压私有池包，把其中的 `frpc-nodes.json` 上传为 `/opt/nscl-relay/relay-data/frpc-nodes.json`。新服务只使用节点池，不需要账号或开房码文件。
3. 在 1Panel 终端修正服务用户的读取权限：

```sh
cd /opt/nscl-relay
chown 10001:10001 relay-data relay-data/*.json
chmod 700 relay-data
chmod 600 relay-data/*.json
curl --fail http://127.0.0.1:17630/v1/frpc/health
```

健康检查应显示 `nodes: 4`、`tunnels: 28`、`max_hosts: 28`。服务每次请求重新读取池文件，已运行时无需重启；首次配置或服务尚未启动时，在部署目录执行 `docker compose up -d`。尚未安装验证服务时，先使用完整的 1Panel 部署包，单独的池文件不能代替服务程序。

再用验证域名检查：

```sh
curl --fail https://你的验证域名/v1/frpc/health
```

启动器默认使用 `https://auth.minguncle.cn`，备用为 `https://auth.northstarmc.cn`，点击「测试联机服务」可检查后端能力。两个域名应指向同一份服务实例。FRP 节点域名由池分配。无需开房码或账号登记，游戏开房端口固定为 25565，最多同时开放 28 个房间。

池文件包含平台认证信息，只存放在私有服务目录；不要放入网站静态目录、Git 或公开下载链接。

## 从本地重新打包

在项目目录运行：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/Package-Relay.ps1 -IncludePrivateCatalog -PoolOnly
```

结果写入 Git 忽略的 `target/relay-packages/`。压缩包内包含池 JSON 和这份说明，旁边另存 SHA-256 校验文件。
