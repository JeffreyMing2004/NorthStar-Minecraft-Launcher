# 启动器黑屏与图形诊断

Windows 默认使用 OpenGL（glow）渲染启动器，其他系统默认使用 wgpu。两个后端均包含在程序中，可在创建窗口前通过参数切换；皮肤模型和其他界面使用同一套 egui 绘制逻辑。

```powershell
.\nscl-vm-compat-debug.exe --renderer=glow
.\nscl-vm-compat-debug.exe --renderer=wgpu
```

调试版在 EXE 所在目录的 `logs/nscl-startup.log` 追加图形诊断。目录不可写时尝试 `%TEMP%\NSCL\nscl-startup.log`。日志包括启动版本、所选后端、图形驱动信息、图形初始化及首帧 UI 构建标记；图形框架仅记录 Info、Warn、Error，不记录账号和 HTTP 模块。

`first UI frame built` 表示 CPU 侧已构建界面，不代表虚拟显卡已经正确显示。窗口仍黑屏时，保留日志并使用另一个后端复测；记录客体 Windows、VMware、VMware Tools 版本及 3D 加速状态。图形初始化失败会显示系统错误窗口，而非只向隐藏控制台输出。

VMware 测试应先安装 VMware Tools 并开启虚拟机的 3D 加速。启动器黑屏应先诊断启动器后端；游戏启动后的 OpenGL 错误使用实例的 `logs/nscl-launch.log` 和 `logs/latest.log` 排查，两者是不同的验证路径。

框架依据：[eframe 渲染配置](https://docs.rs/eframe/0.37.0/eframe/enum.Renderer.html)。VMware 的客体图形支持与宿主机图形后端并不等同，支持范围见 [VMware Workstation 官方介绍](https://www.vmware.com/products/desktop-hypervisor/workstation-and-fusion)。是否解决具体虚拟机的黑屏，需要在对应客体环境实测确认。
