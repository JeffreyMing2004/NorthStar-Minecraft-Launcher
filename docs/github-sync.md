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

1. 打开 <https://cnb.cool/new/repos>，**仓库类型选「密钥仓库」**，名字随意，例如 `ci-credentials`。
2. 建好后进入该仓库，Web 界面新建文件 `github.yml`（密钥仓库禁止 clone，只能在网页上编辑），内容照抄 [`docs/ci/github.yml`](ci/github.yml)，把 `GITHUB_TOKEN` 换成真实 token：

```yaml
GITHUB_USERNAME: JeffreyMing
GITHUB_TOKEN: ghp_xxxxxxxxxxxxxxxx

allow_slugs: JeffreyMing/NorthStar-Minecraft-Launcher
allow_branches: "**"
allow_events: "push,tag_push,web_trigger_sync"
allow_images: "tencentcom/git-sync"
```

3. 打开这个文件的页面，复制地址栏 URL，形如
   `https://cnb.cool/JeffreyMing/ci-credentials/-/blob/main/github.yml`。
4. 在 `.cnb.yml` 里，把三个 pipeline `env` 下的 `CNB_SECRETS_IMPORTS` 都填成这个地址：

```yaml
env:
  CNB_SECRETS_IMPORTS: https://cnb.cool/JeffreyMing/ci-credentials/-/blob/main/github.yml
```

`imports` 会把 YAML 的键值注入为环境变量。**地址留空时 `imports` 是空串，不会报错**，同步任务只是拿不到凭据而失败。

### 方式 B：仓库环境变量（最省事）

在 CNB 仓库页 `设置 → 环境变量` 里直接加 `GITHUB_USERNAME`、`GITHUB_TOKEN`，不用改 `.cnb.yml`。

密钥仓库的优势是引用有审计日志、可跨仓库复用，因此长期看更推荐 A。

## 三、引用权限（最容易卡住的一步）

默认**只有密钥仓库的管理员/负责人**触发的流水线才能引用里面的文件，别人推代码触发同步会直接报无权限。

所以 `github.yml` 里必须声明 `allow_*` 放行规则。一旦写了 `allow_*`，平台就只按规则校验，不再看触发者角色。

```yaml
allow_slugs: JeffreyMing/NorthStar-Minecraft-Launcher   # 只允许本仓库
allow_branches: "**"                                     # 全部分支
allow_events: "push,tag_push,web_trigger_sync"           # 与 .cnb.yml 的触发事件对应
allow_images: "tencentcom/git-sync"                      # 必须：文件被第三方镜像任务引用
```

三条容易漏的：

- `allow_images` 必须写。`github.yml` 被 `tencentcom/git-sync` 这个**镜像任务**引用，没写平台会按「插件任务引用」直接拒绝。镜像名不带 tag，匹配的是 `tencentcom/git-sync` 或 `tencentcom/git-sync:latest`。
- `allow_events` 必须写。`push`、`tag_push`、`web_trigger_sync` 都是用户可控事件（不可信事件），没写同样拒绝。
- `allow_slugs` 里多级组织路径要用 `**`，`*` 不跨 `/`。

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

## 五、常见故障

### `remote: Repository not found` / `Failed to push branch master`

```
remote: Repository not found.
fatal: repository 'https://github.com/JeffreyMing/NorthStar-Minecraft-Launcher.git/' not found
```

GitHub 对「仓库不存在」和「没权限访问」返回**同一个 404**，故意不区分，所以这个报错有两种可能，按顺序排查：

1. **目标仓库压根没建**（最常见）。同步任务只往已有的空仓库里推，不会替你建仓库。
   打开 <https://github.com/new> 新建 `NorthStar-Minecraft-Launcher`，**不要**勾 README / .gitignore / License，保持空仓库，然后重跑流水线。
2. **Token 没权限**。token 所属账号必须对目标仓库有写权限：
   - 经典 token：勾 `repo`（私有仓库）或 `public_repo`（公开仓库）；且 token 未过期、未被吊销。
   - 细粒度 token：`Repository access` 里要显式选中该仓库（新建仓库易漏），权限给 `Contents: Read and write`。
   - 注意 token 属于**哪个账号**：`GITHUB_USERNAME` 必须与该 token 的属主一致。

自己验证一下（在本地执行，能直接暴露是哪种 404）:

```bash
# 不带凭据：404 = 私有或不存在；200 = 公开仓库存在
curl -s -o /dev/null -w "%{http_code}\n" https://github.com/JeffreyMing/NorthStar-Minecraft-Launcher

# 带 token：404 = token 无权或无仓库；200 = 通了
curl -s -o /dev/null -w "%{http_code}\n" \
  -H "Authorization: Bearer $GITHUB_TOKEN" \
  https://api.github.com/repos/JeffreyMing/NorthStar-Minecraft-Launcher
```

改完仓库或 token 后，用页面「同步到 GitHub」按钮（`web_trigger_sync`）补推，不必等下一次 push。

### `Missing or invalid credentials` / 401

`imports` 没取到 `GITHUB_USERNAME`、`GITHUB_TOKEN`。检查 `.cnb.yml` 里 `CNB_SECRETS_IMPORTS` 是否指向**真实存在**的密钥文件，以及密钥仓库文件里的变量名是否一字不差。

### `Permission denied` / 引用密钥文件被拒

密钥仓库里的 `allow_slugs` / `allow_branches` / `allow_events` / `allow_images` 没写或写错，见第三节。

## 六、隐私

**不建议**用这套流程同步私有仓库到公开 GitHub 仓库：会把全部提交历史、分支一起推过去。
