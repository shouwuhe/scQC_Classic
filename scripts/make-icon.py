#!/usr/bin/env python3
"""生成 macOS 应用图标（build/resources/icon.iconset/*.png，再由 iconutil 打成 icon.icns）。

设计：靛蓝圆角方底 + 实心白色液滴，液滴内部挖出三个圆点（细胞）。
全是实心块面，没有细线，因此在 16 px 的 Dock 与访达列表里依然清晰；
"液滴里的细胞"正是这个工具处理的对象。

用法：python3 scripts/make-icon.py [输出目录]
"""
import math
import os
import sys

from PIL import Image, ImageDraw

OUT = sys.argv[1] if len(sys.argv) > 1 else "build/resources/icon.iconset"
S = 1024          # 逻辑尺寸
SS = 4            # 超采样
BG = (72, 82, 214, 255)
BG_TOP = (104, 114, 240, 255)


def teardrop(cx, cy, w, h, n=400):
    """经典泪滴曲线 x=cos t, y=sin t·sin²(t/2)，旋转成尖端朝上。"""
    pts = []
    for i in range(n + 1):
        t = i / n * 2 * math.pi
        u = math.cos(t)
        v = math.sin(t) * (math.sin(t / 2) ** 2)
        pts.append((cx + v * w, cy - u * h))
    return pts


def build(size):
    k = SS
    W = S * k
    img = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    # 背景圆角方 + 自上而下的轻微渐变
    grad = Image.new("RGBA", (W, W))
    gd = ImageDraw.Draw(grad)
    for y in range(W):
        f = y / W
        c = tuple(int(BG_TOP[i] + (BG[i] - BG_TOP[i]) * f) for i in range(3)) + (255,)
        gd.line([(0, y), (W, y)], fill=c)
    mask = Image.new("L", (W, W), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, W - 1, W - 1), radius=int(0.225 * W), fill=255)
    img.paste(grad, (0, 0), mask)

    # 白色实心液滴
    cx, cy = W * 0.5, W * 0.505
    pts = teardrop(cx, cy, W * 0.30, W * 0.335)
    d.polygon(pts, fill=(255, 255, 255, 255))

    # 挖出三个"细胞"（用背景色画实心圆，视觉上是负形）
    def hole(dx, dy, r):
        col = tuple(int(BG_TOP[i] + (BG[i] - BG_TOP[i]) * ((cy + dy * W) / W)) for i in range(3)) + (255,)
        d.ellipse(
            (cx + dx * W - r * W, cy + dy * W - r * W, cx + dx * W + r * W, cy + dy * W + r * W),
            fill=col,
        )

    hole(-0.072, 0.082, 0.050)
    hole(0.068, 0.048, 0.038)
    hole(-0.004, -0.048, 0.031)

    return img.resize((size, size), Image.LANCZOS)


os.makedirs(OUT, exist_ok=True)
for base, scale in [(16, 1), (16, 2), (32, 1), (32, 2), (128, 1), (128, 2), (256, 1), (256, 2), (512, 1), (512, 2)]:
    name = f"icon_{base}x{base}{'@2x' if scale == 2 else ''}.png"
    build(base * scale).save(os.path.join(OUT, name))
build(512).save(os.path.join(os.path.dirname(OUT) or ".", "icon-512.png"))
build(64).save(os.path.join(os.path.dirname(OUT) or ".", "icon-64.png"))
print("wrote", OUT)
