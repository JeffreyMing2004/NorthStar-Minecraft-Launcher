# 启动器黑屏与图形诊断

Windows 在 SMBIOS 固件中识别到 VMware 时默认使用 WARP CPU 软件渲染；其他 Windows 环境默认使用 OpenGL（glow），其他系统默认使用 wgpu。可在创建窗口前通过参数覆盖，皮肤模型和其他界面使用同一套 egui 绘制逻辑。固件识别只读，不记录原始固件内容；识别失败时可显式选择软件模式。

```powershell
.\nscl-vm-compat-debug.exe --renderer=glow
.\nscl-vm-compat-debug.exe --renderer=wgpu
.\nscl-vm-software-debug.exe --renderer=software
```

软件模式只接受支持当前窗口的 DX12 CPU 适配器，通常为 `Microsoft Basic Render Driver`（WARP），不选择 VMware SVGA3D 等硬件适配器。找不到软件适配器时显示错误，避免悄悄回到已出现黑屏的路径。软件绘制速度取决于 CPU，窗口功能保持一致。WARP 的工作方式见 [Microsoft 官方说明](https://learn.microsoft.com/en-us/windows/win32/direct3darticles/directx-warp)。

软件模式的调试构建会在图形日志所在目录保存首帧窗口内部截图 `nscl-software-frame.png`，日志同时记录分辨率和采样颜色数量。截图只包含启动器窗口；该自动截图只用于调试构建。内部截图正常而屏幕窗口仍黑，说明还需要排查呈现或合成；内部截图也异常时继续排查绘制路径。复测可一并提供截图和日志。

调试版在 EXE 所在目录的 `logs/nscl-startup.log` 追加图形诊断。目录不可写时尝试 `%TEMP%\NSCL\nscl-startup.log`。日志包括启动版本、所选后端、图形驱动信息、图形初始化及首帧 UI 构建标记；图形框架仅记录 Info、Warn、Error，不记录账号和 HTTP 模块。

`first UI frame built` 表示 CPU 侧已构建界面，不代表虚拟显卡已经正确显示。窗口仍黑屏时，保留日志并使用另一个后端复测；记录客体 Windows、VMware、VMware Tools 版本及 3D 加速状态。图形初始化失败会显示系统错误窗口，而非只向隐藏控制台输出。

VMware 测试应先安装 VMware Tools 并开启虚拟机的 3D 加速。启动器黑屏应先诊断启动器后端；游戏启动后的 OpenGL 错误使用实例的 `logs/nscl-launch.log` 和 `logs/latest.log` 排查，两者是不同的验证路径。

框架依据：[eframe 渲染配置](https://docs.rs/eframe/0.37.0/eframe/enum.Renderer.html)。VMware 的客体图形支持与宿主机图形后端并不等同，支持范围见 [VMware Workstation 官方介绍](https://www.vmware.com/products/desktop-hypervisor/workstation-and-fusion)。是否解决具体虚拟机的黑屏，需要在对应客体环境实测确认。
