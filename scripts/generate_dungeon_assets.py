#!/usr/bin/env python3
"""Original Embervault assets. Regenerate with Python + fonttools (font only).
No input images, fonts, samples, or downloaded art are used. Outputs are MIT.
"""
import math
from pathlib import Path
import struct
import wave
import zlib

ROOT = Path(__file__).resolve().parents[1] / 'examples/games/assets/dungeon'
ROOT.mkdir(parents=True, exist_ok=True)


def png(path, width, height, pixels):
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    raw = b''.join(b'\0' + bytes(pixels[y * width * 4:(y + 1) * width * 4]) for y in range(height))
    path.write_bytes(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(raw)) + chunk(b'IEND', b''))


# Eight 16px frames per actor: idle, two walk, three attack, two hurt.
W, H = 128, 48
pixels = [0] * (W * H * 4)
def rect(x, y, w, h, color):
    for yy in range(y, y + h):
        for xx in range(x, x + w):
            at = (yy * W + xx) * 4
            pixels[at:at + 4] = color

for actor in range(3):
    for frame in range(8):
        x, y = frame * 16, actor * 16
        bob = int(frame in (2, 4, 7))
        cloak = [(60, 163, 162, 255), (156, 71, 113, 255), (168, 95, 65, 255)][actor]
        dark = (20, 29, 43, 255)
        light = (255, 215, 153, 255) if actor == 0 else (246, 136, 104, 255)
        rect(x + 4, y + 4 + bob, 8, 9 - bob, dark)
        rect(x + 3, y + 8, 10, 4, cloak)
        rect(x + 5, y + 3 + bob, 7, 7, cloak)
        rect(x + 6, y + 5 + bob, 5, 3, dark)
        rect(x + 8, y + 6 + bob, 1, 1, light)
        rect(x + 10, y + 6 + bob, 1, 1, light)
        rect(x + 5, y + 12, 2, 2 + int(frame == 1), dark)
        rect(x + 10, y + 12, 2, 2 + int(frame == 2), dark)
        rect(x + 4, y + 9, 2, 3, light)
        if actor == 0:
            rect(x + 2, y + 9, 2, 3, (250, 177, 71, 255))
        if frame in (3, 4, 5):
            rect(x + 12, y + 3 + (frame - 3) * 3, 3, 2, (237, 231, 208, 255))
        if actor == 2:
            rect(x + 4, y + 1, 2, 4, light)
            rect(x + 10, y + 1, 2, 4, light)
png(ROOT / 'actors.png', W, H, pixels)
# Palette atlas is declared by the level files; tiles are rendered geometrically.
png(ROOT / 'tiles.png', 16, 16, list((47, 57, 66, 255)) * 256)

# Original 5x7 monospaced outline font, with lowercase mapped to uppercase forms.
forms = {
'A':'01110 10001 10001 11111 10001 10001 10001',
'B':'11110 10001 10001 11110 10001 10001 11110',
'C':'01111 10000 10000 10000 10000 10000 01111',
'D':'11110 10001 10001 10001 10001 10001 11110',
'E':'11111 10000 10000 11110 10000 10000 11111',
'F':'11111 10000 10000 11110 10000 10000 10000',
'G':'01111 10000 10000 10111 10001 10001 01111',
'H':'10001 10001 10001 11111 10001 10001 10001',
'I':'11111 00100 00100 00100 00100 00100 11111',
'J':'00111 00010 00010 00010 10010 10010 01100',
'K':'10001 10010 10100 11000 10100 10010 10001',
'L':'10000 10000 10000 10000 10000 10000 11111',
'M':'10001 11011 10101 10101 10001 10001 10001',
'N':'10001 11001 11001 10101 10011 10011 10001',
'O':'01110 10001 10001 10001 10001 10001 01110',
'P':'11110 10001 10001 11110 10000 10000 10000',
'Q':'01110 10001 10001 10001 10101 10010 01101',
'R':'11110 10001 10001 11110 10100 10010 10001',
'S':'01111 10000 10000 01110 00001 00001 11110',
'T':'11111 00100 00100 00100 00100 00100 00100',
'U':'10001 10001 10001 10001 10001 10001 01110',
'V':'10001 10001 10001 10001 10001 01010 00100',
'W':'10001 10001 10001 10101 10101 10101 01010',
'X':'10001 10001 01010 00100 01010 10001 10001',
'Y':'10001 10001 01010 00100 00100 00100 00100',
'Z':'11111 00001 00010 00100 01000 10000 11111',
'0':'01110 10001 10011 10101 11001 10001 01110',
'1':'00100 01100 00100 00100 00100 00100 01110',
'2':'01110 10001 00001 00010 00100 01000 11111',
'3':'11110 00001 00001 01110 00001 00001 11110',
'4':'00010 00110 01010 10010 11111 00010 00010',
'5':'11111 10000 10000 11110 00001 00001 11110',
'6':'01110 10000 10000 11110 10001 10001 01110',
'7':'11111 00001 00010 00100 01000 01000 01000',
'8':'01110 10001 10001 01110 10001 10001 01110',
'9':'01110 10001 10001 01111 00001 00001 01110',
'?':'01110 10001 00001 00010 00100 00000 00100',
'!':'00100 00100 00100 00100 00100 00000 00100',
'.':'00000 00000 00000 00000 00000 00110 00110',
':':'00000 00110 00110 00000 00110 00110 00000',
'-':'00000 00000 00000 11111 00000 00000 00000',
'/':'00001 00001 00010 00100 01000 10000 10000',
'+':'00000 00100 00100 11111 00100 00100 00000',
'=':'00000 00000 11111 00000 11111 00000 00000',
'[':'01110 01000 01000 01000 01000 01000 01110',
']':'01110 00010 00010 00010 00010 00010 01110',
'%':'11001 11010 00010 00100 01000 01011 10011',
"'":'00100 00100 00000 00000 00000 00000 00000',
',':'00000 00000 00000 00000 00110 00110 00100',
';':'00000 00110 00110 00000 00110 00110 00100',
'(':'00010 00100 01000 01000 01000 00100 00010',
')':'01000 00100 00010 00010 00010 00100 01000',
'<':'00001 00010 00100 01000 00100 00010 00001',
'>':'10000 01000 00100 00010 00100 01000 10000',
'_':'00000 00000 00000 00000 00000 00000 11111',
' ':'00000 00000 00000 00000 00000 00000 00000',
}
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
fb = FontBuilder(1000, isTTF=True)
order = ['.notdef'] + [f'u{n}' for n in range(32, 127)]
fb.setupGlyphOrder(order)
fb.setupCharacterMap({n: f'u{n}' for n in range(32, 127)})
glyphs = {}
for name in order:
    char = chr(int(name[1:])) if name != '.notdef' else '?'
    pen = TTGlyphPen(None)
    for y, row in enumerate(forms.get(char.upper(), forms['?']).split()):
        for x, bit in enumerate(row):
            if bit == '1':
                xx, yy = x * 100 + 50, (6 - y) * 100
                pen.moveTo((xx, yy)); pen.lineTo((xx, yy + 100))
                pen.lineTo((xx + 100, yy + 100)); pen.lineTo((xx + 100, yy)); pen.closePath()
    glyphs[name] = pen.glyph()
fb.setupGlyf(glyphs)
fb.setupHorizontalMetrics({g: (650, 50) for g in order})
fb.setupHorizontalHeader(ascent=800, descent=-200)
fb.setupNameTable({'familyName':'Ember Mono', 'styleName':'Regular', 'uniqueFontIdentifier':'EmberMono-1', 'fullName':'Ember Mono', 'psName':'EmberMono', 'version':'Version 1.0'})
fb.setupOS2(sTypoAscender=800, sTypoDescender=-200, usWinAscent=800, usWinDescent=200)
fb.setupPost()
fb.setupMaxp()
fb.font['head'].created = fb.font['head'].modified = 3800000000
fb.save(ROOT / 'ember-mono.ttf')

# Six seamless eight-second ambient loops. Original pentatonic bell figures
# above a soft drone; integer-cycle bass, enveloped notes, no sample inputs.
rate = 16000
for room in range(6):
    samples = []
    base = [110, 130, 146, 164, 130, 110][room]
    notes = [0, 7, 12, 16, 7, 19, 12, 7]
    for i in range(rate * 8):
        t = i / rate
        beat = int(t * 2)
        age = (t * 2) % 1
        hz = base * 2 ** (notes[(beat + room) % len(notes)] / 12)
        bell = math.sin(math.tau * hz * t) * math.exp(-age * 7) * min(age * 90, 1)
        drone = math.sin(math.tau * base * t) * .28
        fade = min(t * 8, (8 - t) * 8, 1)
        samples.append(int((bell * .11 + drone * .10) * fade * 32767))
    with wave.open(str(ROOT / f'room-{room}.wav'), 'wb') as wav:
        wav.setparams((1, 2, rate, 0, 'NONE', 'not compressed'))
        wav.writeframes(struct.pack('<' + 'h' * len(samples), *samples))
for name, hz in [('hit', 165), ('chime', 660), ('dash', 330)]:
    samples = [int(math.sin(math.tau * hz * (i / rate) * (1 - .4 * i / 4000)) * math.exp(-i / 650) * 8000) for i in range(4000)]
    with wave.open(str(ROOT / f'{name}.wav'), 'wb') as wav:
        wav.setparams((1, 2, rate, 0, 'NONE', 'not compressed'))
        wav.writeframes(struct.pack('<' + 'h' * len(samples), *samples))
print(ROOT)
