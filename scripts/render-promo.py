"""Render the 60-second NSCL promo from repository artwork and screenshots.

No launcher build, network access, stock footage, or external audio is needed.
See docs/promo/README.md for dependencies, timing, and reproduction commands.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import random
import subprocess
import sys
import wave

ROOT = Path(__file__).resolve().parents[1]
LOCAL_DEPS = ROOT / "target" / "promo-deps"
if LOCAL_DEPS.is_dir():
    sys.path.insert(0, str(LOCAL_DEPS))

import imageio_ffmpeg
import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont
import qrcode

W, H, FPS, DURATION = 1920, 1080, 30, 60
ANIMATED = "--animated" in sys.argv
MINT = "#B1EDC7"
WHITE = "#F0F5F1"
MUTED = "#99AFA5"
BG = "#0B1415"
DOWNLOAD = "https://github.com/JeffreyMing2004/NorthStar-Minecraft-Launcher/releases"
OUT = ROOT / "docs" / "promo" / ("animated" if ANIMATED else "")
CACHE = ROOT / "target" / ("promo-animated" if ANIMATED else "promo-render")
CAPTURES = ROOT / "docs" / "promo" / "captures"
PREFIX = "NorthStar-Animated" if ANIMATED else "NorthStar"
VIDEO_NAME = f"{PREFIX}-60s-1080p.mp4"
SCENES = [
    (0, 6, "序章", "下一站，你的世界。NorthStar，陪你开启新的冒险。"),
    (6, 14, "开始冒险", "选择实例，管理角色，从熟悉的开始页出发。"),
    (14, 23, "版本安装", "选择游戏版本与加载器，安装、校验和修复一站完成。"),
    (23, 32, "独立实例", "为不同玩法保留独立空间，备份、克隆，回到喜欢的世界。"),
    (32, 41, "内容管理", "发现 Modrinth 内容，管理模组、资源包与光影，导入你的整合包。"),
    (41, 49, "一起联机", "发现局域网世界，检查房间环境，和朋友一起开启冒险。"),
    (49, 54, "从容出发", "自动匹配 Java，自选主题，让每次出发更从容。"),
    (54, 60, "下一站", "NorthStar Minecraft Launcher。下一站，你的世界。"),
]


def font(size: int, bold: bool = False, latin: bool = False):
    windows = Path(os.environ.get("WINDIR", "C:/Windows")) / "Fonts"
    name = ("segoeuib.ttf" if bold else "segoeui.ttf") if latin else (
        "msyhbd.ttc" if bold else "msyh.ttc"
    )
    if not (windows / name).is_file():
        raise FileNotFoundError(f"Required font missing: {windows / name}")
    return ImageFont.truetype(str(windows / name), size)


FONTS = {}


def face(size, bold=False, latin=False):
    key = size, bold, latin
    if key not in FONTS:
        FONTS[key] = font(*key)
    return FONTS[key]


def txt(draw, xy, value, size=30, color=WHITE, bold=False, latin=False, anchor=None):
    draw.text(xy, value, font=face(size, bold, latin), fill=color, anchor=anchor)


def layer():
    return Image.new("RGBA", (W, H), (0, 0, 0, 0))


def star(draw, cx, cy, r, fill=MINT):
    k = r / 4
    draw.polygon([(cx, cy-r), (cx+k, cy-k), (cx+r, cy), (cx+k, cy+k),
                  (cx, cy+r), (cx-k, cy+k), (cx-r, cy), (cx-k, cy-k)], fill=fill)


def pill(draw, x, y, label, accent=False, size=23):
    tw = draw.textlength(label, font=face(size))
    width = int(tw + 36)
    draw.rounded_rectangle((x, y, x+width, y+48), 15,
                           fill="#203B30" if accent else "#172626",
                           outline="#42614F" if accent else "#30433D", width=1)
    txt(draw, (x+18, y+8), label, size, MINT if accent else "#C0D0C7")
    return width + 12


def lines(draw, x, y, values, size=76, step=104):
    for i, value in enumerate(values):
        txt(draw, (x, y+i*step), value, size, MINT if i else WHITE, True)


def background():
    yy, xx = np.mgrid[0:H, 0:W]
    glow = np.exp(-(((xx-1430)/740)**2 + ((yy-350)/620)**2))
    a = np.zeros((H, W, 3), dtype=np.uint8)
    for channel, base in enumerate((9, 17, 18)):
        a[:, :, channel] = base + glow * (10, 25, 20)[channel]
    im = Image.fromarray(a)
    d = ImageDraw.Draw(im)
    for x in range(64, W, 90):
        for y in range(150, 980, 90):
            d.point((x, y), fill="#263931")
    return im.convert("RGBA")


BASE = background()
RNG = random.Random(184)
STARS = [(RNG.randrange(80, W-80), RNG.randrange(150, 945), RNG.uniform(.3, 1.5),
          RNG.uniform(0, math.tau)) for _ in range(44)]


def landscape():
    im = layer()
    d = ImageDraw.Draw(im)
    rng = random.Random(913)
    for row in range(7):
        for col in range(28):
            x = col*85 + row*43 - 330
            y = 790 + row*30 + int(32*math.sin(col*.43))
            h = rng.randrange(14, 90)
            r = 43
            d.polygon([(x, y-h), (x+r, y-h-22), (x+2*r, y-h),
                       (x+r, y-h+22)], fill=(32+row*2, 65+row*2, 53+row*2, 255))
            d.polygon([(x, y-h), (x+r, y-h+22), (x+r, y+40), (x, y+18)],
                      fill=(17, 35+row*2, 30, 255))
            d.polygon([(x+r, y-h+22), (x+2*r, y-h), (x+2*r, y+18), (x+r, y+40)],
                      fill=(23, 45+row*2, 37, 255))
    return im


LAND = landscape()


def screenshot(name, x=746, y=224, width=1080):
    source = CAPTURES if ANIMATED else ROOT / "docs" / "images"
    original = Image.open(source / name).convert("RGBA")
    height = round(original.height * width / original.width)
    shot = original.resize((width, height), Image.Resampling.LANCZOS)
    mask = Image.new("L", (width, height), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, width-1, height-1), 18, fill=255)
    shot.putalpha(mask)
    im = layer()
    shadow = Image.new("RGBA", (width+80, height+80), (0, 0, 0, 0))
    ImageDraw.Draw(shadow).rounded_rectangle((35, 38, width+45, height+48), 28,
                                            fill=(0, 0, 0, 160))
    shadow = shadow.filter(ImageFilter.GaussianBlur(20))
    im.alpha_composite(shadow, (x-40, y-30))
    im.alpha_composite(shot, (x, y))
    d = ImageDraw.Draw(im)
    d.rounded_rectangle((x, y, x+width, y+height), 18, outline="#476255", width=2)
    label = "NSCL  /  实际程序截图" if ANIMATED else "NSCL  /  产品界面"
    txt(d, (x+2, y-42), label, 20, MUTED)
    return im


def note(x, y, title, detail):
    im = layer()
    d = ImageDraw.Draw(im)
    d.rounded_rectangle((x, y, x+520, y+108), 22, fill="#20372E", outline="#5D856B", width=1)
    star(d, x+38, y+51, 13)
    txt(d, (x+70, y+16), title, 27, WHITE, True)
    txt(d, (x+70, y+59), detail, 21, "#AFCCBA")
    return im


def title_layer(kicker, title, desc, tags):
    im = layer()
    d = ImageDraw.Draw(im)
    txt(d, (100, 211), kicker, 23, MINT, latin=True)
    lines(d, 95, 277, title, 73, 102)
    for i, s in enumerate(desc):
        txt(d, (100, 526+i*43), s, 27, "#B0C4B9")
    x, y = 100, 662
    for tag in tags:
        width = int(d.textlength(tag, font=face(23)) + 48)
        if x + width > 665:
            x, y = 100, y+64
        x += pill(d, x, y, tag)
    return im


def make_qr():
    qr = qrcode.QRCode(error_correction=qrcode.constants.ERROR_CORRECT_M, box_size=7, border=4)
    qr.add_data(DOWNLOAD)
    qr.make(fit=True)
    return qr.make_image(fill_color="#102219", back_color="#ECF8EF").convert("RGBA").resize(
        (280, 280), Image.Resampling.NEAREST)


def build_scenes():
    pages = []
    intro = layer()
    d = ImageDraw.Draw(intro)
    txt(d, (104, 239), "NORTHSTAR MINECRAFT LAUNCHER", 26, MINT, latin=True)
    lines(d, 95, 316, ["下一站，", "你的世界。"], 112, 142)
    txt(d, (104, 655), "向着热爱，自由出发。", 32, "#C0D3C6")
    pill(d, 104, 735, "Minecraft Java Edition", True, 24)
    pages.append([(intro, .20, 0, 32)])

    content = [
        ("01  /  YOUR NEXT ADVENTURE", ["每次出发，", "都很简单。"],
         ["游戏实例与角色入口，", "在一个熟悉的开始页。"], ["实例选择", "账号与皮肤"],
         "home.png", "选择热爱的玩法", "选好实例，点击启动游戏"),
        ("02  /  READY TO PLAY", ["想玩的版本，", "轻松就位。"],
         ["游戏与加载器安装，", "从选择版本开始。"], ["Forge", "Fabric", "NeoForge", "Quilt"],
         "downloads.png", "从原版，到更多可能", "安装 · 校验 · 修复"),
        ("03  /  WORLDS, ORGANIZED", ["每个世界，", "各有归属。"],
         ["用独立实例，装下不同玩法。", "也为冒险，留一个恢复点。"], ["独立目录", "内容备份", "实例克隆"],
         "instances.png", "把喜欢的世界留好", "备份内容，保留历史恢复点"),
        ("04  /  MAKE IT YOURS", ["让灵感，", "自由加载。"],
         ["发现 Modrinth 内容，", "为你的世界加入新的可能。"], ["模组", "资源包", "光影", ".mrpack"],
         "instance-files.png", "你的玩法，由你定义", "模组管理 · 整合包导入导出"),
    ]
    for kicker, title, desc, tags, shot, heading, detail in content:
        pages.append([
            (title_layer(kicker, title, desc, tags), .1, 0, 30),
            (screenshot(shot), .28, 65, 12),
            (note(1252, 816, heading, detail), .8, 0, 30),
        ])

    multi = title_layer("05  /  BETTER TOGETHER", ["冒险，", "一起才尽兴。"],
                        ["发现局域网世界，检查房间环境。", "和朋友一起，去更远的地方。"],
                        ["局域网发现", "房间环境检查", "隧道联机"])
    network = layer()
    d = ImageDraw.Draw(network)
    txt(d, (823, 202), "把彼此连接到同一场冒险", 28, "#B6CCBF")
    for cx, cy, label, desc in [(960, 375, "你的世界", "发现局域网世界"),
                               (1610, 375, "朋友加入", "共享房间信息"),
                               (1285, 792, "一起出发", "检查游戏与模组环境")]:
        d.rounded_rectangle((cx-150, cy-77, cx+150, cy+77), 24,
                            fill="#172B24", outline="#456954", width=2)
        txt(d, (cx, cy-41), label, 31, WHITE, True, anchor="mt")
        txt(d, (cx, cy+15), desc, 21, MUTED, anchor="mt")
    txt(d, (1730, 928), "联机功能示意", 19, "#789383", anchor="rt")
    if ANIMATED:
        pages.append([(multi, .1, 0, 30), (screenshot("multiplayer.png"), .28, 65, 12),
                      (note(1252, 816, "从你的世界，到我们的世界", "发现局域网世界 · 连接朋友"), .8, 0, 30)])
    else:
        pages.append([(multi, .1, 0, 30), (network, .4, 0, 30)])

    ease = title_layer("06  /  MADE FOR YOUR JOURNEY", ["把准备，", "交给北极星。"],
                       ["从运行环境，到日常使用。", "让每次出发，更从容。"], ["Windows 10 / 11", "Java 版"])
    cards = layer()
    d = ImageDraw.Draw(cards)
    for j, (number, title, detail) in enumerate([
        ("01", "Rust 原生构建", "Minecraft Java Edition 桌面启动器"),
        ("02", "自动匹配 Java", "按游戏版本选择运行环境"),
        ("03", "选择舒服的界面", "深色 · 浅色 · 高对比度")]):
        y = 240+j*218
        d.rounded_rectangle((825, y, 1795, y+180), 24, fill="#172A24", outline="#365044")
        txt(d, (866, y+39), number, 28, MINT, latin=True)
        txt(d, (946, y+27), title, 39, WHITE, True)
        txt(d, (948, y+100), detail, 25, MUTED)
    pages.append([(ease, .1, 0, 30), (cards, .25, 60, 0)])

    final = layer()
    d = ImageDraw.Draw(final)
    star(d, 142, 260, 41)
    txt(d, (212, 215), "NorthStar", 64, WHITE, True, True)
    txt(d, (215, 296), "MINECRAFT LAUNCHER", 24, MINT, latin=True)
    lines(d, 96, 402, ["下一站，你的世界。"], 88)
    txt(d, (103, 552), "选择你的玩法，现在出发。", 34, "#BDD5C5")
    d.rounded_rectangle((105, 654, 448, 735), 20, fill=MINT)
    txt(d, (276, 672), "下载公开测试版", 30, "#143024", True, anchor="mt")
    txt(d, (106, 781), "开源项目  /  Windows 10 · 11", 26, MUTED)
    # Short repository label plus scannable, exact release URL.
    txt(d, (108, 833), "GitHub  /  JeffreyMing2004", 24, "#BCD3C4", latin=True)
    txt(d, (108, 875), "NorthStar-Minecraft-Launcher", 24, "#BCD3C4", latin=True)
    d.rounded_rectangle((1340, 277, 1740, 779), 30, fill="#172B24", outline="#4E715B", width=2)
    final.alpha_composite(make_qr(), (1400, 321))
    d = ImageDraw.Draw(final)
    txt(d, (1540, 640), "扫码前往下载", 31, WHITE, True, anchor="mt")
    txt(d, (1540, 698), "GitHub Releases", 25, MINT, latin=True, anchor="mt")
    txt(d, (108, 930), "独立非官方项目 · 与 Mojang / Microsoft 无隶属或背书关系", 19, "#80988B")
    pages.append([(final, .12, 0, 28)])
    return pages


PAGES = build_scenes()
PANEL_NAMES = {1: "home.png", 2: "downloads.png", 3: "instances.png",
               4: "instance-files.png", 5: "multiplayer.png"}
CAPTURE_IMAGES = {name: Image.open(CAPTURES / name).convert("RGBA")
                  for name in [*PANEL_NAMES.values(), "home-menu.png", "download-setup.png"]} if ANIMATED else {}
PANEL_MASK = Image.new("L", (1080, 684), 0)
ImageDraw.Draw(PANEL_MASK).rounded_rectangle((0, 0, 1079, 683), 18, fill=255)


def smooth(x):
    x = max(0.0, min(1.0, x))
    return 1-(1-x)**3


def travel(points, local):
    """A short, smooth annotation path over a real captured interface."""
    for (start, a), (end, b) in zip(points, points[1:]):
        if local < end:
            q = smooth((local-start)/(end-start))
            return a[0]+(b[0]-a[0])*q, a[1]+(b[1]-a[1])*q
    return points[-1][1]


def animated_panel(idx, local):
    """Keep the first edit's framing, with small zooms and actual UI states."""
    art = PAGES[idx][1][0].copy()
    screen = CAPTURE_IMAGES[PANEL_NAMES[idx]].copy()
    if idx == 1:
        # The open menu was captured in the running launcher, not recreated.
        weight = smooth((local-2.1)/.32)*(1-smooth((local-4.05)/.30))
        if weight:
            screen = Image.blend(screen, CAPTURE_IMAGES["home-menu.png"], weight)
        points = [(.6, (940, 535)), (2.1, (687, 443)), (4.1, (687, 443)),
                  (5.25, (164, 615)), (8, (164, 615))]
        box = (58, 590, 262, 641) if local > 4.6 else (124, 422, 712, 464)
        click = 2.1
    elif idx == 2:
        weight = smooth((local-3.25)/.38)
        if weight:
            screen = Image.blend(screen, CAPTURE_IMAGES["download-setup.png"], weight)
        points = [(.6, (795, 645)), (3.25, (1077, 312)), (4.2, (1077, 312)),
                  (5.6, (356, 203)), (9, (480, 203))]
        box = (56, 181, 653, 225) if local > 3.8 else (1028, 292, 1127, 336)
        click = 3.25
    elif idx == 3:
        points = [(.6, (160, 451)), (2.6, (160, 451)), (4.2, (595, 440)), (9, (595, 440))]
        box, click = (330, 423, 750, 458), -10
    elif idx == 4:
        points = [(.6, (491, 326)), (2.2, (491, 326)), (4.1, (600, 582)), (9, (600, 582))]
        box, click = (323, 535, 1144, 620), -10
    else:
        points = [(.6, (294, 243)), (3.2, (294, 243)), (5.5, (464, 402)), (8, (464, 402))]
        box = (228, 220, 362, 264) if local < 4.5 else (50, 379, 1149, 423)
        click = -10
    overlay = Image.new("RGBA", screen.size)
    draw = ImageDraw.Draw(overlay)
    if local > 1.1:
        alpha = round((105+55*math.sin(local*2))*smooth((local-1.1)/.5))
        draw.rounded_rectangle(box, 10, outline=(177, 237, 199, alpha), width=2)
        x, y = travel(points, local)
        # A small editorial pointer guides attention; only the two captured
        # menu/navigation interactions receive a click ripple.
        if 0 <= local-click < .48:
            q = (local-click)/.48
            radius = 8+25*q
            draw.ellipse((x-radius, y-radius, x+radius, y+radius),
                         outline=(177, 237, 199, round(220*(1-q))), width=2)
        draw.polygon([(x, y), (x+3, y+23), (x+9, y+16), (x+16, y+28),
                      (x+21, y+25), (x+14, y+14), (x+23, y+12)],
                     fill=(239, 253, 244, 235), outline=(12, 30, 23, 255), width=2)
    screen = Image.alpha_composite(screen, overlay)
    # At most 2.8% enlargement: movement remains subtle and controls readable.
    span = SCENES[idx][1]-SCENES[idx][0]
    zoom = 1+.028*smooth(local/span)
    width, height = screen.size
    cw, ch = width/zoom, height/zoom
    cx = width/2 + 5*math.sin(local*.3)
    cy = height/2 + 3*math.sin(local*.4)
    content = screen.crop((round(cx-cw/2), round(cy-ch/2), round(cx+cw/2), round(cy+ch/2)))
    content = content.resize((1080, 684), Image.Resampling.LANCZOS)
    content.putalpha(PANEL_MASK)
    art.alpha_composite(content, (746, 224))
    ImageDraw.Draw(art).rounded_rectangle((746, 224, 1826, 908), 18,
                                        outline="#476255", width=2)
    return art


def paste_animated(canvas, art, t, delay=0.0, dx=0, dy=0, float_y=0):
    q = smooth((t-delay)/.85)
    if q <= 0:
        return
    x = round(dx*(1-q))
    y = round(dy*(1-q)+float_y)
    if q < .995:
        art = art.copy()
        art.putalpha(art.getchannel("A").point(lambda v: round(v*q)))
    canvas.alpha_composite(art, (x, y))


def decoration(canvas, t, scene):
    d = ImageDraw.Draw(canvas)
    for x, y, size, phase in STARS:
        alpha = int(45+35*math.sin(t*.85+phase))
        c = (int(alpha*.72), alpha, int(alpha*.84), 255)
        yy = y+int(5*math.sin(t*.3+phase))
        d.ellipse((x, yy, x+size+1, yy+size+1), fill=c)
    if scene in (0, 7):
        land = LAND.copy()
        land.putalpha(land.getchannel("A").point(lambda a: int(a*.46)))
        canvas.alpha_composite(land, (int(10*math.sin(t*.12)), 0))
    if scene == 0:
        cx, cy = 1460, 448
        for r in (165, 245, 325):
            d.ellipse((cx-r, cy-r, cx+r, cy+r), outline="#315143", width=1)
        for j in range(3):
            a = t*.12 + j*math.tau/3
            px, py = cx+math.cos(a)*245, cy+math.sin(a)*245
            d.ellipse((px-5, py-5, px+5, py+5), fill=MINT)
        size = 108+4*math.sin(t*1.25)
        star(d, cx, cy, size, MINT)
        star(d, cx-282, cy+206, 14, "#709D80")
        star(d, cx+245, cy-238, 10, "#709D80")
        txt(d, (cx, 851), "FOLLOW YOUR NORTH STAR", 22, "#88AD96", latin=True, anchor="mt")


def network_motion(canvas, local):
    d = ImageDraw.Draw(canvas)
    hub = (1285, 555)
    for end in [(960, 454), (1610, 454), (1285, 715)]:
        q = smooth((local-.4)/1.0)
        d.line([hub, (hub[0]+(end[0]-hub[0])*q, hub[1]+(end[1]-hub[1])*q)],
               fill="#3C6951", width=3)
        for phase in (0, .48):
            p = (local*.23+phase)%1
            x = hub[0]+(end[0]-hub[0])*p*q
            y = hub[1]+(end[1]-hub[1])*p*q
            d.ellipse((x-5, y-5, x+5, y+5), fill=MINT)
    for r in (91, 106+7*math.sin(local*1.2)):
        d.ellipse((hub[0]-r, hub[1]-r, hub[0]+r, hub[1]+r), outline="#385B47", width=2)
    d.ellipse((hub[0]-78, hub[1]-78, hub[0]+78, hub[1]+78), fill="#284C38", outline="#77AC88", width=2)
    star(d, *hub, 44)


def chrome(canvas, global_t, idx):
    d = ImageDraw.Draw(canvas)
    d.rounded_rectangle((90, 49, 139, 98), 13, fill="#223B2D")
    star(d, 114, 73, 16)
    txt(d, (157, 53), "NSCL", 28, WHITE, True, True)
    txt(d, (265, 60), "NORTHSTAR MINECRAFT LAUNCHER", 19, "#809B8D", latin=True)
    txt(d, (1820, 57), f"{idx+1:02d}  /  {SCENES[idx][2]}", 23, "#9FBCAB", anchor="rt")
    d.line((90, 122, 1830, 122), fill="#2A4035", width=1)
    subtitle = SCENES[idx][3]
    if global_t >= SCENES[idx][0]+.8:
        # A fixed safe area keeps subtitles readable throughout transitions.
        tw = d.textlength(subtitle, font=face(27))
        d.rounded_rectangle(((W-tw)/2-20, 976, (W+tw)/2+20, 1021), 10, fill="#0B1614")
        txt(d, (W/2, 980), subtitle, 27, "#D4E4D8", anchor="mt")
    for j, (start, end, _, _) in enumerate(SCENES):
        left = 90 + start/DURATION*1740
        right = 90 + end/DURATION*1740 - 8
        d.rounded_rectangle((left, 1044, right, 1047), 1, fill="#2B4136")
        fill = min(1, max(0, (global_t-start)/(end-start)))
        if fill:
            d.rounded_rectangle((left, 1044, left+(right-left)*fill, 1047), 1, fill=MINT)


def compose_scene(idx, local, global_t):
    im = BASE.copy()
    decoration(im, global_t, idx)
    if idx == 5 and not ANIMATED:
        network_motion(im, local)
    for j, (art, delay, dx, dy) in enumerate(PAGES[idx]):
        if ANIMATED and j == 1 and idx in PANEL_NAMES:
            art = animated_panel(idx, local)
        moving_panels = PANEL_NAMES if ANIMATED else (1, 2, 3, 4)
        drift = math.sin(local*.7)*3 if j == 1 and idx in moving_panels else 0
        paste_animated(im, art, local, delay, dx, dy, drift)
    # Gentle focus ring animates on the real home-page launch button.
    if idx == 1 and local > 2 and not ANIMATED:
        d = ImageDraw.Draw(im)
        pulse = int(2+3*(.5+.5*math.sin(local*2.4)))
        d.rounded_rectangle((794-pulse, 755-pulse, 982+pulse, 808+pulse),
                            15, outline="#83C99B", width=2)
    return im


def frame_at(t):
    idx = next((i for i, (a, b, _, _) in enumerate(SCENES) if a <= t < b), 7)
    start = SCENES[idx][0]
    local = t-start
    im = compose_scene(idx, local, t)
    if idx and local < .36:
        prev = compose_scene(idx-1, SCENES[idx-1][1]-SCENES[idx-1][0]-.02, t)
        im = Image.blend(prev, im, smooth(local/.36))
    chrome(im, t, idx)
    if t < .5:
        im = Image.blend(Image.new("RGBA", (W, H), BG), im, smooth(t/.5))
    if t > 59.65:
        im = Image.blend(im, Image.new("RGBA", (W, H), BG), smooth((t-59.65)/.35)*.65)
    return im.convert("RGB")


def make_music(path):
    """Original deterministic electronic score: D - Bm - G - A, 100 BPM."""
    sr = 48000
    count = DURATION*sr
    audio = np.zeros((count, 2), np.float32)
    rng = np.random.default_rng(60211)

    def tone(start, length, frequencies, gain, pan=0, pluck=False):
        n = min(int(length*sr), count-int(start*sr))
        if n <= 0:
            return
        ts = np.arange(n, dtype=np.float32)/sr
        sig = np.zeros(n, np.float32)
        for freq in frequencies:
            sig += np.sin(math.tau*freq*ts)+.18*np.sin(math.tau*freq*2*ts)
        sig /= len(frequencies)
        if pluck:
            env = (1-np.exp(-ts*160))*np.exp(-ts*4.3)
        else:
            env = np.minimum(ts/.18, 1)*np.minimum((length-ts)/.35, 1)
        sig *= np.maximum(env, 0)*gain
        i = int(start*sr)
        audio[i:i+n, 0] += sig*math.sqrt((1-pan)/2)
        audio[i:i+n, 1] += sig*math.sqrt((1+pan)/2)

    def hz(midi):
        return 440*2**((midi-69)/12)

    beat = .6
    chords = [(50, 54, 57, 61), (47, 50, 54, 57), (43, 47, 50, 54), (45, 49, 52, 57)]
    for bar in range(25):
        chord = chords[bar%4]
        start = bar*beat*4
        tone(start, 2.65, [hz(n) for n in chord], .12, -.25)
        tone(start+.06, 2.6, [hz(n)*1.002 for n in chord], .075, .4)
        for b in range(4):
            tone(start+b*beat, .52, [hz(chord[0]-12)], .11)
        for k in range(8):
            midi = chord[(k if bar%2 == 0 else 7-k)%4]+12
            tone(start+k*beat/2, 1.2, [hz(midi)], .095, -.48 if k%2 else .48, True)

    # Restrained kick and hand-made noise percussion enter after the intro.
    for b in range(10, 94):
        i = int(b*beat*sr)
        n = int(.22*sr)
        ts = np.arange(n)/sr
        kick = np.sin(math.tau*(42*ts+7*(1-np.exp(-ts*22))))*np.exp(-ts*23)*.19
        audio[i:i+n] += kick[:, None]
        if b%2:
            n = int(.12*sr)
            ts = np.arange(n)/sr
            noise = rng.normal(0, 1, n)
            noise[1:] -= .8*noise[:-1]
            clap = noise*np.exp(-ts*45)*.023
            audio[i:i+n] += clap[:, None]
        for off in (0, .3):
            j = int((b*beat+off)*sr)
            n = int(.055*sr)
            ts = np.arange(n)/sr
            noise = rng.normal(0, 1, n)
            noise[1:] -= .92*noise[:-1]
            hat = noise*np.exp(-ts*75)*.012
            audio[j:j+n, 0] += hat*.8
            audio[j:j+n, 1] += hat

    # Soft transition accents follow the edit points.
    for start, _, _, _ in SCENES[1:]:
        tone(start, 2, [hz(74), hz(81)], .045, .12, True)
    t = np.arange(count, dtype=np.float32)/sr
    envelope = np.minimum(t/1.7, 1)*np.minimum((DURATION-t)/2.3, 1)
    audio *= np.maximum(envelope, 0)[:, None]
    peak = float(np.max(np.abs(audio)))
    audio *= .81/max(peak, .001)
    with wave.open(str(path), "wb") as f:
        f.setnchannels(2)
        f.setsampwidth(2)
        f.setframerate(sr)
        f.writeframes((np.clip(audio, -1, 1)*32767).astype("<i2").tobytes())


def stamp(seconds):
    return f"00:{seconds//60:02d}:{seconds%60:02d},000"


def metadata():
    subtitles = "\n\n".join(f"{i+1}\n{stamp(a)} --> {stamp(b)}\n{s}"
                              for i, (a, b, _, s) in enumerate(SCENES))+"\n"
    (OUT / f"{PREFIX}-60s.zh-CN.srt").write_text(subtitles, encoding="utf-8")
    shotlist = [{"in": a, "out": b, "scene": title, "caption": s}
                for a, b, title, s in SCENES]
    (OUT / "storyboard.json").write_text(json.dumps(shotlist, ensure_ascii=False, indent=2)+"\n", encoding="utf-8")


def previews():
    frame_at(3).save(OUT / f"{PREFIX}-cover.jpg", quality=95)
    sheet = Image.new("RGB", (1920, 2160), BG)
    for i, (start, end, title, _) in enumerate(SCENES):
        frame = frame_at((start+end)/2).resize((960, 540), Image.Resampling.LANCZOS)
        sheet.paste(frame, ((i%2)*960, (i//2)*540))
    sheet.save(OUT / f"{PREFIX}-storyboard.jpg", quality=91)
    if ANIMATED:
        detail = Image.new("RGB", (1920, 1080), BG)
        for i, t in enumerate([7.4, 8.8, 16.4, 19.2]):
            detail.paste(frame_at(t).resize((960, 540), Image.Resampling.LANCZOS),
                         ((i%2)*960, (i//2)*540))
        detail.save(OUT / "NorthStar-Animated-interactions.jpg", quality=93)


def render():
    ffmpeg = imageio_ffmpeg.get_ffmpeg_exe()
    music = CACHE / "original-score.wav"
    make_music(music)
    destination = OUT / VIDEO_NAME
    temporary = CACHE / f"{PREFIX}-60s-1080p.partial.mp4"
    command = [ffmpeg, "-y", "-hide_banner", "-loglevel", "warning", "-f", "rawvideo",
               "-pix_fmt", "rgb24", "-s", f"{W}x{H}", "-r", str(FPS), "-i", "pipe:0",
               "-i", str(music), "-map", "0:v:0", "-map", "1:a:0",
               "-c:v", "libx264", "-preset", "fast", "-crf", "19",
               "-pix_fmt", "yuv420p", "-profile:v", "high", "-level:v", "4.1",
               "-threads", "6", "-c:a", "aac", "-b:a", "192k", "-ar", "48000",
               "-af", "loudnorm=I=-16:TP=-1.5:LRA=9", "-t", str(DURATION),
               "-movflags", "+faststart", "-metadata", "title=NorthStar | 下一站，你的世界",
               "-metadata", "comment=Original motion design and synthesized score; NSCL project screenshots.",
               str(temporary)]
    with (CACHE / "encode.log").open("w", encoding="utf-8") as log:
        proc = subprocess.Popen(command, stdin=subprocess.PIPE, stderr=log)
        try:
            for n in range(DURATION*FPS):
                proc.stdin.write(frame_at(n/FPS).tobytes())
                if n%150 == 0:
                    print(f"Rendered {n//FPS:02d}/{DURATION} seconds", flush=True)
            proc.stdin.close()
            status = proc.wait()
        except BaseException:
            proc.kill()
            proc.wait()
            raise
    if status:
        raise RuntimeError(f"FFmpeg failed ({status}); see {CACHE / 'encode.log'}")
    temporary.replace(destination)
    print(f"Exported {destination} ({destination.stat().st_size/1024**2:.1f} MiB)", flush=True)


def verify():
    video = OUT / VIDEO_NAME
    ffmpeg = imageio_ffmpeg.get_ffmpeg_exe()
    # Full decode catches damaged frames and invalid timestamps, not only headers.
    result = subprocess.run([ffmpeg, "-v", "error", "-i", str(video), "-f", "null", "-"],
                            capture_output=True, text=True, check=True)
    if result.stderr.strip():
        raise RuntimeError(result.stderr)
    reader = imageio_ffmpeg.read_frames(str(video), pix_fmt="rgb24")
    info = next(reader)
    frames = sum(1 for _ in reader)
    assert info["size"] == (W, H), info
    assert abs(info["fps"]-FPS) < .001, info
    assert abs(info["duration"]-DURATION) < .05, info
    assert frames == FPS*DURATION, frames
    assert info.get("audio_codec") == "aac", info
    # Validate the declared destination, edit continuity, and subtitle safe area.
    assert DOWNLOAD.endswith("/NorthStar-Minecraft-Launcher/releases")
    assert len(SCENES) == 8 and SCENES[0][0] == 0 and SCENES[-1][1] == DURATION
    assert all(a[1] == b[0] for a, b in zip(SCENES, SCENES[1:]))
    for _, _, _, caption in SCENES:
        assert ImageDraw.Draw(BASE).textlength(caption, font=face(27)) < 1720, caption
    digest = hashlib.sha256(video.read_bytes()).hexdigest()
    report = {"file": video.name, "duration_seconds": info["duration"],
              "resolution": list(info["size"]), "fps": info["fps"],
              "decoded_frames": frames, "video_codec": info.get("codec"),
              "audio_codec": info.get("audio_codec"), "full_decode": "passed",
              "download_url": DOWNLOAD, "sha256": digest}
    if ANIMATED:
        report["source"] = "Fresh screenshots of the running NSCL debug application"
        report["capture_count"] = len(CAPTURE_IMAGES)
        report["style"] = "Original dark mint layout with subtle motion"
    (OUT / "verification.json").write_text(json.dumps(report, ensure_ascii=False, indent=2)+"\n", encoding="utf-8")
    print(json.dumps(report, ensure_ascii=False, indent=2), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--preview-only", action="store_true")
    parser.add_argument("--verify-only", action="store_true")
    parser.add_argument("--animated", action="store_true",
                        help="Use fresh program captures and subtle original-style animation")
    args = parser.parse_args()
    OUT.mkdir(parents=True, exist_ok=True)
    CACHE.mkdir(parents=True, exist_ok=True)
    if args.verify_only:
        verify()
        return
    metadata()
    previews()
    if not args.preview_only:
        render()
        verify()


if __name__ == "__main__":
    main()
