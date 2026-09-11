#!/usr/bin/env python3
"""生成 DuoSwitch 应用图标（1024×1024 PNG）。

主题呼应 iPhone Duo「沙丘+山峰」壁纸：左半日间、右半夜间，共用同一条沙丘山脊。
生成后用 `cargo tauri icon src-tauri/icons/source-icon.png` 产出各平台尺寸。

依赖 Pillow（用 PYTHONPATH 指向已有的 _pylibs 目录即可）：
    PYTHONPATH=<...>/_pylibs python make-icon.py
"""
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

SIZE = 1024
HALF = SIZE // 2
OUT = Path(__file__).resolve().parent.parent / "src-tauri" / "icons" / "source-icon.png"

# 日间：天蓝 → 地平线暖霞
DAY_TOP = (122, 176, 226)
DAY_BOTTOM = (255, 219, 168)
# 夜间：深靛 → 暮紫
NIGHT_TOP = (14, 21, 51)
NIGHT_BOTTOM = (58, 52, 96)

# 日/夜沙丘色
DAY_SAND = (206, 158, 108)
NIGHT_SAND = (28, 32, 62)

# 沙丘/山脊轮廓，横跨左右两半，保证中缝连续
RIDGE = [
    (0, 706),
    (128, 636),
    (268, 692),
    (402, 618),
    (512, 662),
    (640, 596),
    (768, 656),
    (896, 604),
    (1024, 648),
]


def vertical_gradient(width: int, height: int, top: tuple[int, int, int], bottom: tuple[int, int, int]) -> Image.Image:
    """先画 1 像素宽的渐变条再放大，避免逐像素循环。"""
    strip = Image.new("RGB", (1, height))
    px = strip.load()
    for y in range(height):
        t = y / (height - 1)
        px[0, y] = tuple(round(top[i] + (bottom[i] - top[i]) * t) for i in range(3))
    return strip.resize((width, height), Image.BILINEAR)


def half_mask(left: bool) -> Image.Image:
    m = Image.new("L", (SIZE, SIZE), 0)
    box = (0, 0, HALF - 1, SIZE - 1) if left else (HALF, 0, SIZE - 1, SIZE - 1)
    ImageDraw.Draw(m).rectangle(box, fill=255)
    return m


def dune_layer(color: tuple[int, int, int], mask: Image.Image) -> Image.Image:
    """沙丘填充块，alpha 先按左右半区裁掉，避免覆盖另一侧的装饰元素。"""
    layer = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    ImageDraw.Draw(layer).polygon([*RIDGE, (SIZE, SIZE), (0, SIZE)], fill=(*color, 255))
    layer.putalpha(ImageChops.multiply(layer.getchannel("A"), mask))
    return layer


def sun() -> Image.Image:
    out = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    glow = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    ImageDraw.Draw(glow).ellipse((150, 320, 470, 640), fill=(255, 206, 118, 140))
    out.alpha_composite(glow.filter(ImageFilter.GaussianBlur(58)))
    disc = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    ImageDraw.Draw(disc).ellipse((190, 360, 430, 600), fill=(255, 226, 152, 255))
    out.alpha_composite(disc)
    return out


def moon() -> Image.Image:
    mask = Image.new("L", (SIZE, SIZE), 0)
    md = ImageDraw.Draw(mask)
    md.ellipse((600, 300, 850, 550), fill=255)
    md.ellipse((556, 268, 796, 508), fill=0)
    mask = mask.filter(ImageFilter.GaussianBlur(1.2))

    disc = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    disc.paste((236, 241, 252, 255), mask=mask)

    out = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    glow = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    ImageDraw.Draw(glow).ellipse((590, 290, 860, 560), fill=(170, 196, 255, 100))
    out.alpha_composite(glow.filter(ImageFilter.GaussianBlur(52)))
    out.alpha_composite(disc)
    return out


def main() -> None:
    bg = Image.new("RGB", (SIZE, SIZE))
    bg.paste(vertical_gradient(HALF, SIZE, DAY_TOP, DAY_BOTTOM), (0, 0))
    bg.paste(vertical_gradient(HALF, SIZE, NIGHT_TOP, NIGHT_BOTTOM), (HALF, 0))

    canvas = bg.convert("RGBA")
    canvas.alpha_composite(sun())
    canvas.alpha_composite(moon())
    canvas.alpha_composite(dune_layer(DAY_SAND, half_mask(left=True)))
    canvas.alpha_composite(dune_layer(NIGHT_SAND, half_mask(left=False)))

    # 圆角方形遮罩
    rounded = Image.new("L", (SIZE, SIZE), 0)
    ImageDraw.Draw(rounded).rounded_rectangle((0, 0, SIZE - 1, SIZE - 1), radius=210, fill=255)
    canvas.putalpha(rounded)

    OUT.parent.mkdir(parents=True, exist_ok=True)
    canvas.save(OUT)
    print(f"{OUT}  {canvas.size[0]}x{canvas.size[1]}  {OUT.stat().st_size} bytes")


if __name__ == "__main__":
    main()
