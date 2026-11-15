from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

directory = Path('crates/handheld-ui/fonts')
font = ImageFont.truetype(directory / 'Rajdhani-SemiBold.ttf', 256)
height = 48
width = 26
advance = round(max(font.getlength(char) for char in '0123456789'))
top = min(font.getbbox(char, anchor='ls')[1] for char in '0123456789')
bottom = max(font.getbbox(char, anchor='ls')[3] for char in '0123456789')
bitmap = bytearray()
for char in '0123456789.-_':
    canvas = Image.new('L', (advance, bottom - top))
    x = round((advance - font.getlength(char)) / 2)
    ImageDraw.Draw(canvas).text((x, -top), char, font=font, fill=255, anchor='ls')
    glyph = canvas.resize((width, height), Image.Resampling.LANCZOS)
    if char == '_':
        ImageDraw.Draw(glyph).rectangle((3, 44, 22, 46), fill=255)
    for y in range(height):
        row = [glyph.getpixel((x, y)) for x in range(width)] + [0, 0]
        for start in range(0, len(row), 4):
            bitmap.append(sum(((value + 42) // 85) << (2 * index)
                              for index, value in enumerate(row[start:start + 4])))
(directory / 'rajdhani-frequency-2bpp.bin').write_bytes(bitmap)
