# GitHub 与 CNB 双仓库推送

GitHub 仓库为 [JeffreyMing2004/NorthStar-Minecraft-Launcher](https://github.com/JeffreyMing2004/NorthStar-Minecraft-Launcher)，CNB 仓库为 [JeffreyMing/NorthStar-Minecraft-Launcher](https://cnb.cool/JeffreyMing/NorthStar-Minecraft-Launcher)，默认分支均为 `master`。

## 本地提交与推送

先使用 GitHub CLI 登录并配置 Git 凭据：

```powershell
gh auth login --hostname github.com
gh auth setup-git --hostname github.com
git remote add github https://github.com/JeffreyMing2004/NorthStar-Minecraft-Launcher.git
git remote add origin https://cnb.cool/JeffreyMing/NorthStar-Minecraft-Launcher
```

已有远程时使用 `git remote set-url` 更新地址。每完成一项项目修改，执行相应检查，只暂存相关文件，再提交并推送两个远程：

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
git add <本次相关文件>
git commit -m "准确概括本次修改"
git push github master
git push origin master
```

GitHub CLI 负责登录及 Git 凭据，提交由 `git commit` 完成；`gh` 没有提交子命令。推送使用正常快进，不使用强制推送。任一远程失败时保留本地提交，检查网络或先拉取并合并远程变更后重试，避免覆盖历史。没有改动时不创建空提交。

`AGENTS.md` 只留本地，通过 `.git/info/exclude` 的 `/AGENTS.md` 排除；设置、账号、令牌、下载的游戏与构建产物都不加入提交。

## CNB 后备同步

`.cnb.yml` 保留 CNB 到 GitHub 的同步任务，作为后备同步入口。目标地址与本地 `github` 相同，`force: false`，遇到分叉会失败并保留 GitHub 历史。本地已直接推送时通常无需额外同步。

流水线凭据从现有 CNB 密钥仓库文件导入：`https://cnb.cool/JeffreyMing/ci-github/-/blob/main/github.yml`。模板见 [docs/ci/github.yml](ci/github.yml)，`GITHUB_USERNAME` 应为 GitHub 账号 `JeffreyMing2004`，`GITHUB_TOKEN` 由仓库维护者保存在密钥仓库；本项目只保留占位模板。

密钥文件中的 `allow_slugs` 使用 CNB 路径 `JeffreyMing/NorthStar-Minecraft-Launcher`，并允许现有的 `push,tag_push,web_trigger_sync` 事件及 `tencentcom/git-sync` 镜像。若更换密钥文件地址，应同时修改 `.cnb.yml` 中三处 `CNB_SECRETS_IMPORTS`。凭据需要允许写入目标仓库内容；同步 GitHub 工作流还需具备对应工作流写入权限。

普通 push 同步当前分支，tag push 同步标签；CNB 页面手动入口用于补推分支和标签。GitHub Releases 由 GitHub 的发布工作流构建，详见[启动器更新](launcher-updates.md)。
