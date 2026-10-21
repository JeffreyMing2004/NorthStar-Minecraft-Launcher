# CNB → GitHub 自动同步

代码推到 CNB 后，由 CNB 流水线自动推一份到 GitHub，以 CNB 为主仓库、GitHub 为镜像。

## 一、准备 GitHub 凭据

1. 打开 GitHub → Settings → Developer settings → Personal access tokens。
2. 新建 token：
   - 经典 token：勾 `repo`（公开仓库只需 `public_repo`）。
   - 细粒度 token：仅授权目标仓库，权限给 `Contents: Read and write`。
3. 记下用户名（`GITHUB_USERNAME`）和 token（`GITHUB_TOKEN`）。

## 二、在 CNB 存密钥

推荐用**密钥仓库**（禁止本地克隆、引用有审计）：

1. 在 <https://cnb.cool/new/repos> 选「密钥仓库」新建，例如 `JeffreyMing/ci-secrets`。
2. 在 Web 界面新建 `github.yml`：

```yaml
GITHUB_USERNAME: JeffreyMing
GITHUB_TOKEN: ghp_xxxxxxxxxxxxxxxx
```

3. 回到本仓库 `.cnb.yml`，在需要同步的 pipeline 上加 `imports`：

```yaml
$:
  push:
    - imports:
        - https://cnb.cool/JeffreyMing/ci-secrets/-/blob/main/github.yml
      name: sync-to-github
      # ...
```

不想建密钥仓库时，也可直接在仓库「设置 → 环境变量」里加这两个变量，
但密钥仓库有引用审计，更推荐前者。

## 三、行为说明

| 场景 | 行为 |
| --- | --- |
| 任意分支 `push` | 把当前分支强制推到 GitHub 同名分支 |
| 推 `tag` | 推送标签到 GitHub |
| 页面「同步到 GitHub」按钮 | 全量推所有分支 + 标签，用于补推历史 |

- `force: true`：CNB 为准，GitHub 上同名分支会被覆盖。**不要在 GitHub 侧直接改代码**，否则会被冲掉。
- 只推当前分支通过 `branch: $CNB_BRANCH` 实现；去掉这一行则推所有远端跟踪分支。
- 换目标仓库改 `.cnb.yml` 里的 `GITHUB_SYNC_URL` 即可，不必改其它地方。
- CNB 能直连 GitHub；若推送超时，检查构建节点网络出站策略。

## 四、隐私

**不建议**用这套流程同步私有仓库到公开 GitHub 仓库：会把全部提交历史、分支一起推过去。
