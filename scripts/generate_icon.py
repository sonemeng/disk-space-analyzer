#!/usr/bin/env python3
"""Generate the release PNG and a multi-resolution Windows ICO."""

from pathlib import Path

from PIL import Image, ImageDraw


CANVAS = 1024


def vertical_gradient(size: int, top: str, bottom: str) -> Image.Image:
    image = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    top_rgb = ImageColor.getrgb(top)
    bottom_rgb = ImageColor.getrgb(bottom)
    pixels = image.load()
    for y in range(size):
        ratio = y / max(size - 1, 1)
        color = tuple(round(top_rgb[i] * (1 - ratio) + bottom_rgb[i] * ratio) for i in range(3))
        for x in range(size):
            pixels[x, y] = (*color, 255)
    return image


# Kept as a small local shim so the drawing code remains easy to read.
class ImageColor:
    @staticmethod
    def getrgb(value: str) -> tuple[int, int, int]:
        value = value.lstrip("#")
        return tuple(int(value[index:index + 2], 16) for index in (0, 2, 4))


def make_source() -> Image.Image:
    image = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    mask = Image.new("L", image.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle((62, 62, 962, 962), radius=224, fill=255)
    gradient = vertical_gradient(CANVAS, "#3182F6", "#1DB9B0")
    image.alpha_composite(Image.composite(gradient, Image.new("RGBA", image.size), mask))
    draw = ImageDraw.Draw(image)

    # Bright corner accent gives the mark a recognizable dopamine-color signature.
    draw.rounded_rectangle((680, 64, 962, 346), radius=116, fill="#FF6B6B")
    draw.ellipse((728, 112, 914, 298), fill="#FFB347")

    # Hard-drive body and subtle lower shadow.
    draw.rounded_rectangle((210, 235, 814, 800), radius=92, fill="#145AA4")
    draw.rounded_rectangle((190, 210, 794, 775), radius=92, fill="#FFFFFF")
    draw.rounded_rectangle((190, 650, 794, 775), radius=42, fill="#EAF4FF")
    draw.line((190, 650, 794, 650), fill="#C9DCF2", width=16)

    # Capacity ring: three bright segments around a dark center.
    ring_box = (300, 280, 684, 664)
    draw.arc(ring_box, start=210, end=355, fill="#3182F6", width=58)
    draw.arc(ring_box, start=5, end=108, fill="#12A47B", width=58)
    draw.arc(ring_box, start=118, end=198, fill="#FFB347", width=58)
    draw.ellipse((432, 412, 552, 532), fill="#344054")
    draw.ellipse((468, 448, 516, 496), fill="#FFFFFF")

    # Bottom slot and live status light stay visible down to 16px.
    draw.rounded_rectangle((270, 700, 590, 730), radius=15, fill="#9CBFE5")
    draw.ellipse((674, 686, 738, 750), fill="#FF5D5D")
    return image


def generate_assets(project_root: Path) -> tuple[Path, Path]:
    assets = project_root / "assets"
    assets.mkdir(parents=True, exist_ok=True)
    source_path = assets / "icon-5.1.png"
    ico_path = assets / "icon.ico"
    source = make_source()
    source.save(source_path, optimize=True)
    source.save(
        ico_path,
        format="ICO",
        sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )
    return source_path, ico_path


if __name__ == "__main__":
    root = Path(__file__).resolve().parent.parent
    png, ico = generate_assets(root)
    print(f"PNG: {png} ({png.stat().st_size} bytes)")
    print(f"ICO: {ico} ({ico.stat().st_size} bytes)")
