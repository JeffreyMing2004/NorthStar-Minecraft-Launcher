# CNB → GitHub 自动同步

代码推到 CNB 后，由 CNB 流水线自动推一份到 GitHub，以 CNB 为主仓库、GitHub 为镜像。

## 一、准备 GitHub 凭据

1. 打开 GitHub → Settings → Developer settings → Personal access tokens。
2. 新建 token：
   - 经典 token：勾 `repo`（公开仓库只需 `public_repo`）。
   - 细粒度 token：仅授权目标仓库，权限给 `Contents: Read and write`。
3. 记下用户名（`GITHUB_USERNAME`）和 token（`GITHUB_TOKEN`）。

## 二、在 CNB 存密钥

两条路，任选其一。**没有密钥仓库也能先用**，凑齐凭据后同步才会成功。

### 方式 A：密钥仓库（推荐）

密钥仓库不是现成的入口，要自己新建一个：

1. 打开 <https://cnb.cool/new/repos>，**仓库类型选「密钥仓库」**，名字随意，例如 `ci-secrets`。
2. 建好后进入该仓库，在 Web 界面新建文件 `github.yml`（密钥仓库禁止 clone，只能在网页上编辑）：

```yaml
GITHUB_USERNAME: JeffreyMing
GITHUB_TOKEN: ghp_xxxxxxxxxxxxxxxx
```

3. 打开文件页面，复制地址栏里的 URL，形如
   `https://cnb.cool/JeffreyMing/ci-secrets/-/blob/main/github.yml`。
4. 在 `.cnb.yml` 里，把每个 pipeline `env` 下的 `CNB_SECRETS_IMPORTS` 填成这个地址：

```yaml
env:
  CNB_SECRETS_IMPORTS: https://cnb.cool/JeffreyMing/ci-secrets/-/blob/main/github.yml
```

`imports` 会把这个 YAML 的键值注入为环境变量。**地址留空时 `imports` 是空串，不会报错**，同步任务只是拿不到凭据而失败。

### 方式 B：仓库环境变量（最省事）

在 CNB 仓库页 `设置 → 环境变量` 里直接加 `GITHUB_USERNAME`、`GITHUB_TOKEN`，不用改 `.cnb.yml`。

密钥仓库的优势是引用有审计日志、可跨仓库复用，因此长期看更推荐 A。

## 三、密钥仓库的引用权限（容易踩）

默认**只有密钥仓库的管理员/负责人**触发的流水线才能引用里面的文件。别人推代码触发同步时会报无权限。

解决办法是在 `github.yml` 里声明放行范围：

```yaml
GITHUB_USERNAME: JeffreyMing
GITHUB_TOKEN: ghp_xxxxxxxxxxxxxxxx
allow_slugs: JeffreyMing/NorthStar-Minecraft-Launcher
allow_branches: "**"
allow_events: "push,tag_push,web_trigger_sync"
```

- `allow_slugs`：只允许本仓库引用（多级组织路径要用 `**`）
- `allow_branches`：允许的分支，`**` 代表全部
- `allow_events`：允许的触发事件，对应 `.cnb.yml` 里的 `push` / `tag_push` / `web_trigger_sync`

一旦写了 `allow_*`，平台就只按规则校验，不再看触发者角色。

## 四、行为说明

| 场景 | 行为 |
| --- | --- |
| 任意分支 `push` | 把当前分支强制推到 GitHub 同名分支 |
| 推 `tag` | 推送标签到 GitHub |
| 页面「同步到 GitHub」按钮 | 全量推所有分支 + 标签，用于补推历史 |

- `force: true`：CNB 为准，GitHub 上同名分支会被覆盖。**不要在 GitHub 侧直接改代码**，否则会被冲掉。
- 只推当前分支通过 `branch: $CNB_BRANCH` 实现；去掉这一行则推所有远端跟踪分支。
- 换目标仓库改 `.cnb.yml` 里的 `GITHUB_SYNC_URL` 即可，不必改其它地方。
- CNB 能直连 GitHub；若推送超时，检查构建节点网络出站策略。

## 五、隐私

**不建议**用这套流程同步私有仓库到公开 GitHub 仓库：会把全部提交历史、分支一起推过去。
