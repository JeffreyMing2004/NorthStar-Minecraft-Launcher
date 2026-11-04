# NorthStar · 第一版风格的轻动画版

成片：`NorthStar-Animated-60s-1080p.mp4`，60 秒，1920 × 1080，30 fps，H.264 / AAC 立体声，保留第一版的深色薄荷绿、左右排版、八个分镜、中文字幕和原创配乐。

画面改用本次运行调试版启动器获取的实际截图，详见 [截图说明](../captures/README.md)。开始页展示真实菜单展开和收起，下载页过渡到实际加载器选择界面，并加入小幅缩放、指针引导、点击涟漪、功能高亮和原有的星点与品牌动画。联机段使用真实联机入口。指针和强调边框为后期说明效果，不表示执行了游戏启动、资源安装或创建房间。

## 重新生成

依赖与第一版相同，见上级 `requirements.txt`。无需重新编译启动器，也无需再次运行桌面程序；截图素材已保存在 `docs/promo/captures`。

```powershell
python scripts/render-promo.py --animated --preview-only
python scripts/render-promo.py --animated
python scripts/render-promo.py --animated --verify-only
```

中间文件位于已忽略的 `target/promo-animated`。`NorthStar-Animated-storyboard.jpg` 是八个分镜总览，`NorthStar-Animated-interactions.jpg` 对比菜单及版本选择前后状态；另附封面、字幕、分镜时间表与完整解码校验报告。

第一版及 Fluent 版本仍保留在各自原有路径；不带 `--animated` 的渲染命令继续生成第一版。
