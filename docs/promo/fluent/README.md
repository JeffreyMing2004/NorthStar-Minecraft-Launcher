# NorthStar · Fluent 风格宣传片

`NorthStar-Fluent-60s-1080p.mp4`：60 秒、1920 × 1080、30 fps，H.264 / AAC 立体声。新版本保存在此目录，第一版仍位于上级 `docs/promo`。

本版以 Windows 11 发布宣传片及其设计资料为视觉参考，采用明亮薄荷色空间、曲面绽放、圆角立体品牌标志、窗口透视运镜、层叠界面、柔和光影与简洁标题。三维曲面、材质、动画、剪辑和合成配乐由本项目脚本生成。功能展示使用项目已有的真实截图；联机画面标明为功能示意。成片没有旁白，标题已合成到画面，另提供中文字幕 SRT。

参考：[Introducing Windows 11](https://www.youtube.com/watch?v=Uh9643c2P6k)、[Windows 11: Designing the next generation](https://microsoft.design/articles/windows-11-designing-the-next-generation/)、[Windows 11 blossoms with Bloom](https://blogs.windows.com/windowsexperience/2021/10/06/windows-11-blossoms-with-bloom-a-new-symbol-for-a-new-operating-system/)。视频不包含这些参考中的音视频素材、Windows 壁纸或 Windows 标志。

## 分镜

| 时间 | 内容 |
| --- | --- |
| 00:00–00:06 | 薄荷色曲面由近到远展开，「让热爱，有个新起点」 |
| 00:06–00:10 | 四角星立体标志出现 |
| 00:10–00:17 | 开始页窗口进入空间，轻推镜头 |
| 00:17–00:23 | 下载页与加载器卡片 |
| 00:23–00:30 | 实例页与多窗口层叠展开 |
| 00:30–00:37 | 模组文件页与内容卡片 |
| 00:37–00:44 | 联机节点与沿路径运动的连接光点 |
| 00:44–00:50 | Java、主题、Rust 三张立体卡片 |
| 00:50–00:54 | 深色品牌空间，「你的玩法，你的世界」 |
| 00:54–01:00 | 品牌、口号与 GitHub Releases 下载二维码 |

## 生成与检查

需要 Windows、Microsoft Edge、Node.js 22+ 和 Python 3.12+。使用系统 Microsoft YaHei / Segoe UI 字体。依赖和渲染缓存均位于项目已忽略的 `target` 目录；不构建启动器。

```powershell
python -m pip install --target target/promo-deps -r docs/promo/requirements.txt
npm install --prefix target/promo-web --ignore-scripts --no-audit --no-fund three@0.180.0 playwright@1.62.1
node scripts/render-promo-fluent.mjs --preview-only
node scripts/render-promo-fluent.mjs
```

渲染器优先使用 `target/promo-web/node_modules` 中的 Playwright；在 Codex 桌面环境中也可使用已配置的运行库。Python 可通过 `CODEX_PYTHON` 指定，默认优先使用桌面运行库，没有该运行库时使用 PATH 中的 `python`。

第一次安装依赖需要网络，后续渲染只访问临时开启的本机 HTTP 服务。Edge 使用全新的临时浏览器配置，渲染过程不访问个人浏览器资料。渲染结束后自动关闭浏览器和本机服务。

`scene.html` / `scene.mjs` 为可复现的三维画面源文件。`window.renderAt(seconds)` 直接定位任意画面；全部动画按时间计算，不使用实际等待时间。`scripts/render-promo-fluent.mjs` 逐帧捕获画面并交给 FFmpeg；`scripts/promo-fluent-media.py` 生成原创合成配乐、封面、分镜图及校验报告。片尾二维码指向项目 GitHub Releases。

渲染结束后完整解码视频，核对 1800 帧、60 秒、1080p、30 fps、H.264 / AAC，并记录 SHA-256 到 `verification.json`。`NorthStar-Fluent-storyboard.jpg` 提供十个分镜的总览。单独验证已有成片：

```powershell
python scripts/promo-fluent-media.py --verify
```
