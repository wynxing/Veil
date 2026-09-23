"""Render the simple Veil vector geometry at Windows icon sizes."""

from pathlib import Path
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent
SIZES = (16, 24, 32, 48, 64, 128, 256)
SCALE = 8


def render(size: int, holding: bool = False) -> Image.Image:
    canvas = Image.new("RGBA", (64 * SCALE, 64 * SCALE), (0, 0, 0, 0))
    draw = ImageDraw.Draw(canvas)
    s = lambda points: [(int(x * SCALE), int(y * SCALE)) for x, y in points]
    box = lambda xy: tuple(int(v * SCALE) for v in xy)
    navy = "#142c34"
    veil = "#c44830" if holding else "#0b8c94"
    fold = "#903421" if holding else "#076b75"
    draw.rounded_rectangle(box((5, 9, 59, 49)), radius=6 * SCALE, fill=navy)
    draw.rounded_rectangle(box((10, 14, 54, 44)), radius=2 * SCALE, fill="#e9f8f8")
    draw.polygon(s([(31, 14), (54, 14), (54, 39), (48, 37), (42, 33), (37, 27)]), fill=veil)
    draw.polygon(s([(43, 14), (54, 14), (54, 38), (49, 35), (46, 29)]), fill=fold)
    draw.rectangle(box((28, 48, 36, 54)), fill=navy)
    draw.rounded_rectangle(box((17, 54, 47, 59)), radius=2 * SCALE, fill=navy)
    return canvas.resize((size, size), Image.Resampling.LANCZOS)


def main() -> None:
    idle = [render(n) for n in SIZES]
    idle[-1].save(ROOT / "veil.ico", format="ICO", sizes=[(n, n) for n in SIZES], append_images=idle[:-1])
    render(64).save(ROOT / "veil.png")
    render(16).save(ROOT / "tray-idle.png")
    render(16, holding=True).save(ROOT / "tray-holding.png")


if __name__ == "__main__":
    main()
