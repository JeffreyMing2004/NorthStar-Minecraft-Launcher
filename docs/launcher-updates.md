# 启动器更新与发布

## 获取更新

启动器从公开仓库 [JeffreyMing2004/NorthStar-Minecraft-Launcher 的 Releases](https://github.com/JeffreyMing2004/NorthStar-Minecraft-Launcher/releases) 获取最新正式版本，不需要登录 GitHub 或填写访问令牌。默认启动时后台检查，可在「更多」关闭启动检查或手动重试。

版本按语义化版本号比较，只提示比当前程序新的稳定版本，跳过草稿和预发布，不自动降级。没有正式 Release 时显示「GitHub 暂无已发布的正式 Release」；网络或 API 限流失败会显示错误并保留当前程序。

在「更多」点击「下载更新」，下载适配当前架构的 Windows 便携 ZIP 或 EXE。资产名称需以 `nscl` 开始，包含 `windows` 和 `x86_64`（也接受 `x64`）或 `aarch64`，不使用 `setup` / `installer` 安装包。ZIP 内必须包含唯一的 `NSCL.exe`，只读取该文件，不解压其他内容。

资源必须来自上述仓库的 HTTPS 发布路径，下载校验 GitHub 资产 `digest` 提供的 SHA-256；缺少该字段时读取同一 Release 的 `SHA256SUMS`。未提供校验、校验失败、取消或程序架构不符时均不执行替换。API 与资产字段依据 [GitHub Releases REST API](https://docs.github.com/en/rest/releases/releases)。

## 重启、备份与回退

退出游戏，完成或取消后台安装、账号登录和皮肤操作后，点击「重启并更新」。辅助进程等待启动器退出，再检查下载文件与原程序是否变化，在原目录创建新文件并将旧程序备份为 `.nscl-backup-<随机 ID>.exe`，最后替换并启动新版。文件替换或创建进程失败时尝试恢复旧版；原目录不可写时保留原程序并提示错误，不请求管理员权限。

更新不会修改设置、账号、MC 路径或实例数据。缓存及诊断保存在设置文件旁的 `cache/updates/`；失败原因写入 `last-error.txt`，再次打开「更多」可以查看。若新版本成功启动后出现运行问题，可退出启动器，备份当前 EXE，再将保留的旧版 EXE 复制回 `NSCL.exe` 后运行。自动回退仅覆盖替换及创建进程失败，不判断新版本运行期间的功能问题。

当前内置替换流程面向 Windows；其他系统可打开发布页手动获取。启动器本身及游戏默认保存在可写目录时可直接替换；装在受保护目录时应手动更新或移至用户可写目录。

## 发布构建

`.github/workflows/release.yml` 在推送 `v*` 标签时执行格式、Clippy、自动测试及 Windows x86_64 Release 构建，然后通过 GitHub CLI 创建 Release，附带：

- `nscl-v<版本>-windows-x86_64.zip`，内含 `NSCL.exe`。
- `SHA256SUMS`，UTF-8 格式的 SHA-256 清单。

发布标签必须与 `Cargo.toml` 中的稳定版本完全一致，例如 `v0.1.1` 对应 `version = "0.1.1"`。维护者先完成版本号及锁文件更新、检查和双仓库推送，再创建标签并推送到两个远程。工作流失败时不会创建 Release。手动运行工作流只上传构建产物，不发布版本。

首次正式发布前，客户端会显示暂无 Release；普通源码推送不会产生可安装的更新。更新源使用公开 Release，不从分支或 Actions 产物获取程序。
