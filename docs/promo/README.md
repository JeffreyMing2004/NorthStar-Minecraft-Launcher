# NorthStar 一分钟宣传片

新增的 Windows 11 / Fluent 风格版本见 [fluent/README.md](fluent/README.md)，成片为 `fluent/NorthStar-Fluent-60s-1080p.mp4`。下文保留第一版的说明与生成方式。

成片：`NorthStar-60s-1080p.mp4`，60 秒，1920 × 1080，30 fps，H.264 / AAC 立体声。中文字幕直接合成到画面，并提供 UTF-8 的 `NorthStar-60s.zh-CN.srt`。视频为原创动态图形、项目界面截图与原创合成电子配乐，无旁白。

主题为「下一站，你的世界」，沿用 NSCL 的深色与薄荷绿视觉。片尾二维码指向项目 GitHub Releases；公开测试版的安装方式及限制以仓库发布说明为准。联机画面明确标注「联机功能示意」。

| 时间 | 内容 |
| --- | --- |
| 00:00–00:06 | 四角星、方块地形与品牌开场 |
| 00:06–00:14 | 真实开始页，实例与角色入口 |
| 00:14–00:23 | 真实下载页，游戏与加载器安装 |
| 00:23–00:32 | 真实实例页，独立目录、备份与克隆 |
| 00:32–00:41 | 真实文件页，模组、资源包、光影与整合包 |
| 00:41–00:49 | 联机功能示意，世界发现与房间环境检查 |
| 00:49–00:54 | Rust、Java 自动匹配与主题选择 |
| 00:54–01:00 | 品牌收尾与下载二维码 |

## 重新生成

使用 Windows 上的 Python 3.12+。渲染器使用系统 Microsoft YaHei / Segoe UI 字体；字体不打包进仓库。首次安装 `imageio-ffmpeg` 时需联网获取包含编码器的 wheel，后续渲染完全离线。安装到已被项目忽略的 `target`：

```powershell
python -m pip install --target target/promo-deps -r docs/promo/requirements.txt
python scripts/render-promo.py --preview-only
python scripts/render-promo.py
```

此流程不编译启动器。中间音频、编码日志和临时成片位于 `target/promo-render`。画面采用固定随机种子，音乐由脚本直接合成，不依赖外部音源、素材服务或语音服务。

`NorthStar-cover.jpg` 是封面，`NorthStar-storyboard.jpg` 是八个分镜的总览。`storyboard.json` 记录字幕与时间点。`verification.json` 记录时长、分辨率、帧数、编码格式及 SHA-256。

## 检查

渲染完成后自动完整解码视频，核对 60 秒、1080p、30 fps、1800 帧和 AAC 音轨，并检查字幕宽度和分镜连续性。也可单独运行：

```powershell
python scripts/render-promo.py --verify-only
```

发布前可检查分镜总览，并播放视频确认画面、转场和音量。宣传文案依据仓库 `README.md`、`docs/user-guide.md` 与界面截图；没有宣传尚未启用的 CurseForge 查询，也没有声称自动解决任意模组兼容问题。

本片使用仓库已有的启动器截图及品牌四角星形状。Minecraft 相关名称与截图内素材归各自权利人所有；NSCL 为独立非官方项目，片尾保留相应说明。
