# 界面素材

## 项目标志

`nscl-logo.svg` 是 README 使用的 NSCL 标志，沿用启动器的四角星与深绿配色。

`nscl-icon.png` 与多尺寸 `nscl.ico` 使用相同图形。PNG 随程序嵌入，用于窗口、任务栏和 Alt+Tab；ICO 通过 `nscl.rc` 与 `build.rs` 写入 Windows 可执行文件资源，用于资源管理器及快捷方式。调试版也包含这些图标。

版本管理中没有本地图片的实例使用程序绘制的分类图标，不依赖网络或第三方图标素材。

## 默认角色纹理

`steve.png` 来自 Minecraft Java Edition 1.20.1 客户端资源中的
`assets/minecraft/textures/entity/player/wide/steve.png`，用于未登录、离线账号或未获得账号皮肤时的标准 Steve 预览。
纹理随程序嵌入，显示默认角色不需要联网或先安装 Minecraft。
