# 故障排查

## 网络与下载

- 下载连接失败或校验不通过时，自动下载源会依次尝试 BMCLAPI、minguncle 和官方源。版本列表刷新失败会保留已有结果，可通过重试重新加载。
- 默认跟随系统代理。代理无法连接时，可在设置中选择直连，或使用错误窗口提供的「切换直连并重试」。这不会修改 Windows 的系统代理配置。
- 错误窗口提供原因、操作建议和可复制的技术详情。安装、导入等会修改文件的操作需要手动确认后重试。
- Windows 错误 [10013](https://learn.microsoft.com/en-us/windows/win32/winsock/windows-sockets-error-codes-2) 表示本地连接访问被拒绝，不是 Mojang 返回的 HTTP 状态码。请检查程序网络权限、代理和安全软件；更换下载源不能解决本地权限限制。
- 请从普通桌面终端运行程序。隔离或受限执行环境中的程序可能继承网络访问限制。

## 界面黑屏与图形诊断

Windows 在 SMBIOS 固件中识别到 VMware 时默认使用 WARP CPU 软件渲染；其他 Windows 环境默认使用 OpenGL（glow），其他系统默认使用 wgpu。可在创建窗口前通过参数覆盖，皮肤模型和其他界面使用同一套 egui 绘制逻辑。固件识别只读，不记录原始固件内容；识别失败时可显式选择软件模式。

在程序所在目录运行以下命令；源码构建的程序位于 `target/debug/`。请使用实际的可执行文件名。

```powershell
.\nscl.exe --renderer=glow
.\nscl.exe --renderer=wgpu
.\nscl.exe --renderer=software
```

软件模式只接受支持当前窗口的 DX12 CPU 适配器，通常为 `Microsoft Basic Render Driver`（WARP），不选择 VMware SVGA3D 等硬件适配器。找不到软件适配器时显示错误，避免悄悄回到已出现黑屏的路径。软件绘制速度取决于 CPU，窗口功能保持一致。WARP 的工作方式见 [Microsoft 官方说明](https://learn.microsoft.com/en-us/windows/win32/direct3darticles/directx-warp)。

软件模式的调试构建会在图形日志所在目录保存首帧窗口内部截图 `nscl-software-frame.png`，日志同时记录分辨率和采样颜色数量。截图只包含启动器窗口；该自动截图只用于调试构建。内部截图正常而屏幕窗口仍黑，说明还需要排查呈现或合成；内部截图也异常时继续排查绘制路径。复测可一并提供截图和日志。

调试版在 EXE 所在目录的 `logs/nscl-startup.log` 追加图形诊断。目录不可写时尝试 `%TEMP%\NSCL\nscl-startup.log`。日志包括启动版本、所选后端、图形驱动信息、图形初始化及首帧 UI 构建标记；图形框架仅记录 Info、Warn、Error，不记录账号和 HTTP 模块。

`first UI frame built` 表示 CPU 侧已构建界面，不代表虚拟显卡已经正确显示。窗口仍黑屏时，保留日志并使用另一个后端复测；记录客体 Windows、VMware、VMware Tools 版本及 3D 加速状态。图形初始化失败会显示系统错误窗口，而非只向隐藏控制台输出。

VMware 测试应先安装 VMware Tools 并开启虚拟机的 3D 加速。启动器黑屏应先诊断启动器后端；游戏启动后的 OpenGL 错误使用实例的 `logs/nscl-launch.log` 和 `logs/latest.log` 排查，两者是不同的验证路径。

框架依据：[eframe 渲染配置](https://docs.rs/eframe/0.37.0/eframe/enum.Renderer.html)。VMware 的客体图形支持与宿主机图形后端并不等同，支持范围见 [VMware Workstation 官方介绍](https://www.vmware.com/products/desktop-hypervisor/workstation-and-fusion)。是否解决具体虚拟机的黑屏，需要在对应客体环境实测确认。

## 提交问题

游戏运行过后，可在启动页点击「查看日志」，查看启动日志和同目录的 `latest.log` 末尾 64 KiB。诊断会列出匹配到的常见错误、对应原始日志行和处理建议；这些是可能原因，`latest.log` 也可能来自上一次运行。没有匹配时请继续查看原始日志。

「导出脱敏诊断包」会创建新的 ZIP 文件，只包含固定的错误特征、来源和处理建议，不包含原始日志、账号或个人路径。为避免泄露未知格式的敏感数据，未识别的日志内容不会导出；如果需要分享完整日志，请先自行检查并脱敏。

通过 [GitHub Issues](https://github.com/JeffreyMing2004/NorthStar-Minecraft-Launcher/issues) 提交问题时，说明启动器版本、操作系统、游戏及加载器版本、复现步骤，并附相关日志。启动器窗口问题附 `nscl-startup.log`；游戏启动问题附游戏目录中的 `logs/nscl-launch.log` 和 `logs/latest.log`。分享前移除日志中的账号、令牌与个人路径等信息。
