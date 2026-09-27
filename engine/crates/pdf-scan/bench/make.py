#!/usr/bin/env python3
import io
import os
import sys
import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont
FONTS = {'eng': '/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf', 'lao': '/usr/share/fonts/truetype/noto/NotoSansLao-Regular.ttf'}
ENGLISH = 'The committee will meet on Friday to review the budget for next year. Please bring the signed forms and a copy of the invoice. Payments are due within thirty days of the date on the invoice. Contact the office if the amount is not correct or if you need more time to pay.'.split()
LAO = 'ກອງປະຊຸມຈະຈັດຂຶ້ນໃນວັນສຸກ ເພື່ອທົບທວນງົບປະມານສຳລັບປີໜ້າ ກະລຸນານຳເອົາແບບຟອມ ທີ່ລົງລາຍເຊັນແລ້ວ ແລະ ສຳເນົາໃບແຈ້ງໜີ້ມານຳ ການຊຳລະເງິນຕ້ອງເຮັດພາຍໃນສາມສິບວັນ'.split()

def page(rng, language, fill=1.0):
    w, h = (1240, 1754)
    img = Image.new('L', (w, h), 250)
    draw = ImageDraw.Draw(img)
    font = ImageFont.truetype(FONTS[language], 30)
    title = ImageFont.truetype(FONTS[language], 46)
    words = LAO if language == 'lao' else ENGLISH
    lines = []
    y = 120
    heading = ' '.join(rng.choice(words, 3))
    draw.text((110, y), heading, font=title, fill=20)
    lines.append(heading)
    y += 110
    while y < min(h - 160, 120 + fill * (h - 280)):
        line = []
        while True:
            word = rng.choice(words)
            trial = ' '.join(line + [word])
            if draw.textlength(trial, font=font) > w - 220:
                break
            line.append(word)
        text = ' '.join(line)
        draw.text((110, y), text, font=font, fill=25)
        lines.append(text)
        y += 52 if rng.random() > 0.15 else 104
    return (img, '\n'.join(lines))

def table(rng, w, h):
    kind = rng.integers(0, 4)
    x = np.linspace(0, 1, w)[None, :]
    yy = np.linspace(0, 1, h)[:, None]
    if kind == 0:
        base = 70 + 25 * np.sin(x * 40 + np.sin(yy * 6) * 3)
    elif kind == 1:
        base = 185 + 12 * np.sin(yy * 9) + rng.normal(0, 3, (h, w))
    elif kind == 2:
        base = 120 + 30 * np.sin(x * 160) * np.sin(yy * 160)
    else:
        base = 100 + 60 * (np.sin(x * 13 + yy * 7) > 0.3)
    colour = rng.uniform(0.7, 1.2, 3)
    rgb = np.stack([base * c for c in colour], axis=-1)
    return np.clip(rgb, 0, 255)
ACCENTS = [(22, 110, 200), (40, 150, 220), (20, 120, 70), (190, 40, 40), (60, 60, 70), (230, 140, 20)]

def dress(rng, doc):
    w, h = doc.size
    tint = (250, 250, 250) if rng.random() < 0.8 else tuple((int(v) for v in rng.uniform(225, 250, 3)))
    grey = np.asarray(doc).astype(float) / 250
    img = Image.fromarray(np.clip(grey[..., None] * np.array(tint), 0, 255).astype(np.uint8))
    draw = ImageDraw.Draw(img)
    accent = tuple(ACCENTS[rng.integers(0, len(ACCENTS))])
    light = tuple((int(255 - (255 - c) * 0.35) for c in accent))
    if rng.random() < 0.8:
        r = rng.uniform(45, 80)
        draw.ellipse((110 - r / 2, 60, 110 + r * 1.5, 60 + r * 2), fill=accent)
        y = rng.uniform(190, 260)
        for k in range(rng.integers(1, 4)):
            start = 0 if rng.random() < 0.5 else rng.uniform(60, 300)
            draw.line((start, y + k * 14, w, y + k * 14 - rng.uniform(0, 60)), fill=accent, width=int(rng.uniform(4, 12)))
    if rng.random() < 0.7:
        top = rng.uniform(0.35, 0.6) * h
        bottom = min(top + rng.uniform(0.2, 0.45) * h, h - 220)
        left, right = (rng.uniform(60, 140), w - rng.uniform(60, 140))
        draw.rectangle((left, top, right, bottom), fill=tint, outline=(40, 40, 40), width=3)
        head = rng.uniform(50, 110)
        draw.rectangle((left, top, right, top + head), fill=light, outline=(40, 40, 40), width=3)
        rows = rng.integers(3, 9)
        for k in range(1, rows):
            yy = top + head + (bottom - top - head) * k / rows
            draw.line((left, yy, right, yy), fill=(40, 40, 40), width=2)
        for x in np.sort(rng.uniform(left + 60, right - 60, rng.integers(1, 4))):
            draw.line((x, top, x, bottom), fill=(40, 40, 40), width=2)
        for k in range(rows * 2):
            yy = top + head + 12 + (bottom - top - head - 30) * k / (rows * 2)
            draw.rectangle((left + 20, yy, left + 20 + rng.uniform(80, right - left - 60), yy + 9), fill=(70, 70, 70))
    if rng.random() < 0.75:
        band = rng.uniform(40, 150)
        lift = rng.uniform(0, 80)
        foot = h - rng.uniform(0, 90)
        draw.polygon([(0, foot - band), (w * 0.6, foot - band), (w, foot - band - lift), (w, foot), (0, foot)], fill=accent)
        for k in range(2):
            yy = foot - band + 12 + k * 20
            draw.rectangle((w * 0.15, yy, w * 0.85, yy + 6), fill=(235, 235, 245))
    if rng.random() < 0.3:
        cx, cy = (rng.uniform(300, w - 300), rng.uniform(200, h - 300))
        ink = (40, 60, 180) if rng.random() < 0.6 else (190, 30, 30)
        draw.ellipse((cx - 110, cy - 110, cx + 110, cy + 110), outline=ink, width=8)
    if rng.random() < 0.2:
        x0, y0 = (rng.uniform(100, w - 500), rng.uniform(300, h - 600))
        draw.rectangle((x0, y0, x0 + rng.uniform(200, 400), y0 + rng.uniform(150, 300)), fill=tuple((int(v) for v in rng.uniform(30, 120, 3))))
    return img

def busy_table(rng, w, h):
    x = np.linspace(0, 1, w)[None, :]
    yy = np.linspace(0, 1, h)[:, None]

    def surface(kind):
        if kind == 0:
            grain = 150 + 30 * np.sin(x * 90 + np.sin(yy * 20) * 2) + rng.normal(0, 4, (h, w))
            return np.stack([grain * 1.25, grain * 0.75, grain * 0.4], -1)
        if kind == 1:
            tile = 215 + rng.normal(0, 4, (h, w))
            size = rng.uniform(0.15, 0.3)
            grout = (np.abs(x / size % 1 - 0.5) > 0.48) | (np.abs(yy / size % 1 - 0.5) > 0.48)
            tile = tile - 40 * grout
            gx, gy = rng.uniform(0, 1, 2)
            tile = tile + 40 * np.exp(-(((x - gx) / 0.1) ** 2 + ((yy - gy) / 0.25) ** 2))
            return np.stack([tile] * 3, -1)
        if kind == 2:
            cloth = 45 + 10 * np.sin(x * 300) * np.sin(yy * 300) + rng.normal(0, 5, (h, w))
            colour = [0.8, 1.2, 0.9] if rng.random() < 0.5 else [1, 1, 1]
            return np.stack([cloth * c for c in colour], -1)
        base = 180 + rng.normal(0, 3, (h, w))
        return np.stack([base * c for c in rng.uniform(0.85, 1.05, 3)], -1)
    kinds = rng.permutation(4)[:rng.integers(2, 4)]
    cuts = np.sort(rng.uniform(0.1, 0.9, len(kinds) - 1))
    slant = rng.uniform(-0.3, 0.3)
    where = x + slant * yy
    out = surface(kinds[0])
    for kind, cut in zip(kinds[1:], cuts):
        out = np.where((where > cut)[..., None], surface(kind), out)
    img = Image.fromarray(np.clip(out, 0, 255).astype(np.uint8))
    draw = ImageDraw.Draw(img)
    for _ in range(rng.integers(0, 4)):
        points = [tuple(rng.uniform(0, 1, 2) * (w, h)) for _ in range(rng.integers(3, 7))]
        draw.line(points, fill=(235, 235, 235) if rng.random() < 0.6 else (20, 20, 20), width=int(rng.uniform(4, 12)), joint='curve')
    if rng.random() < 0.5:
        cx, cy, r = (rng.uniform(0, w), rng.uniform(0, h), rng.uniform(60, 160))
        for k, colour in enumerate([(255, 60, 60), (80, 255, 80), (80, 120, 255), (255, 80, 255)]):
            draw.arc((cx - r, cy - r, cx + r, cy + r), k * 90, k * 90 + 80, fill=colour, width=int(r * 0.15))
    if rng.random() < 0.5:
        cx, cy = (rng.uniform(0, w), rng.uniform(0, h))
        draw.ellipse((cx - 250, cy - 400, cx + 250, cy + 400), fill=tuple((int(v) for v in rng.uniform(10, 50, 3))))
    if rng.random() < 0.4:
        cx, cy = (rng.uniform(0, w), rng.uniform(0, h))
        draw.rectangle((cx, cy, cx + rng.uniform(100, 300), cy + rng.uniform(100, 200)), fill=tuple((int(v) for v in rng.uniform(120, 240, 3))))
    return np.asarray(img).astype(float)

def outdoors(rng, w, h):
    x = np.linspace(0, 1, w)[None, :]
    yy = np.linspace(0, 1, h)[:, None]
    sky = 235 + 15 * rng.random() - 40 * yy + rng.normal(0, 3, (h, w))
    out = np.stack([sky * 0.97, sky, sky * 1.02], -1)
    img = Image.fromarray(np.clip(out, 0, 255).astype(np.uint8))
    draw = ImageDraw.Draw(img)
    for _ in range(rng.integers(3, 9)):
        if rng.random() < 0.6:
            y0 = rng.uniform(0, 0.35) * h
            draw.line((0, y0, w, y0 + rng.uniform(-80, 80)), fill=tuple((int(v) for v in rng.uniform(90, 200, 3))), width=int(rng.uniform(6, 30)))
        else:
            x0 = rng.uniform(0, w)
            draw.line((x0, 0, x0 + rng.uniform(-60, 60), h), fill=tuple((int(v) for v in rng.uniform(80, 210, 3))), width=int(rng.uniform(8, 40)))
    for _ in range(rng.integers(6, 30)):
        cx, cy, r = (rng.uniform(0, w), rng.uniform(0.2, 0.8) * h, rng.uniform(20, 120))
        green = (int(rng.uniform(40, 120)), int(rng.uniform(100, 190)), int(rng.uniform(30, 90)))
        draw.ellipse((cx - r, cy - r * 0.7, cx + r, cy + r * 0.7), fill=green)
    if rng.random() < 0.7:
        top = rng.uniform(0.45, 0.8) * h
        colour = tuple((int(v) for v in rng.uniform(150, 240, 3)))
        draw.rectangle((0, top, w, h), fill=colour)
    if rng.random() < 0.5:
        x0 = rng.uniform(0.5, 0.9) * w
        draw.rectangle((x0, 0, x0 + rng.uniform(60, 250), h), fill=(250, 250, 248))
    img = img.filter(ImageFilter.GaussianBlur(rng.uniform(1, 6)))
    return np.asarray(img).astype(float)

def tiles(rng, w, h):
    x = np.linspace(0, 1, w)[None, :]
    yy = np.linspace(0, 1, h)[:, None]
    tone = rng.uniform(150, 215)
    base = tone + rng.normal(0, 2, (h, w)) - 25 * yy * rng.random()
    size = rng.uniform(0.35, 0.8)
    slant = rng.uniform(-0.3, 0.3)
    u, v = (x + slant * yy, yy - slant * x)
    grout = (np.abs(u / size % 1 - 0.5) > 0.494) | (np.abs(v / size % 1 - 0.5) > 0.494)
    base = base - rng.uniform(15, 50) * grout
    for _ in range(rng.integers(0, 3)):
        gx, gy = (rng.uniform(0, 1), rng.uniform(0, 1))
        base = base + rng.uniform(30, 90) * np.exp(-(((x - gx) / rng.uniform(0.02, 0.15)) ** 2 + ((yy - gy) / rng.uniform(0.03, 0.2)) ** 2))
    tint = np.array([1.0, rng.uniform(0.92, 0.98), rng.uniform(0.78, 0.9)])
    return np.stack([base * c for c in tint], -1)

def photograph(rng, doc, difficulty, busy=False, scene=None):
    W, H = (1440, 1080) if scene == 'wide' else (1080, 1440)
    if scene == 'held':
        background = outdoors(rng, W, H)
    elif scene == 'tiles':
        background = tiles(rng, W, H)
    else:
        background = busy_table(rng, W, H) if busy else table(rng, W, H)
    if scene is None:
        scale = rng.uniform(0.45, 0.62)
        pw, ph = (W * scale * 1.35, H * scale)
        cx, cy = (W / 2 + rng.uniform(-80, 80), H / 2 + rng.uniform(-80, 80))
        angle = rng.uniform(-0.25, 0.25) * (0.5 + difficulty)
    else:
        ph = min(W, H) * rng.uniform(0.55, 0.95) / (0.707 if W > H else 1.0)
        ph = min(ph, H * 0.95) if W <= H else ph
        pw = ph * 0.707
        cx, cy = (W / 2 + rng.uniform(-0.15, 0.15) * W, H / 2 + rng.uniform(-0.12, 0.12) * H)
        angle = rng.uniform(-0.5, 0.5)
        if W > H or rng.random() < 0.1:
            angle += np.pi / 2 * (1 if rng.random() < 0.5 else -1)
            if W > H:
                pw, ph = (pw, min(ph, W * 0.9))
                pw = min(pw, H * 0.95)
                ph = pw / 0.707
    base = np.array([[-pw / 2, -ph / 2], [pw / 2, -ph / 2], [pw / 2, ph / 2], [-pw / 2, ph / 2]])
    rot = np.array([[np.cos(angle), -np.sin(angle)], [np.sin(angle), np.cos(angle)]])
    corners = base @ rot.T
    corners += rng.uniform(-1, 1, (4, 2)) * 70 * difficulty
    corners += [cx, cy]
    src = [(0, 0), (doc.width, 0), (doc.width, doc.height), (0, doc.height)]
    coeffs = perspective(corners, src)
    pageimg = doc.convert('RGB').transform((W, H), Image.PERSPECTIVE, coeffs, Image.BICUBIC, fillcolor=(0, 0, 0))
    mask = Image.new('L', doc.size, 255).transform((W, H), Image.PERSPECTIVE, coeffs, Image.BILINEAR, fillcolor=0)
    paper_tint = rng.uniform(0.85, 1.0, 3)
    if scene == 'held':
        paper_tint = paper_tint * rng.uniform(0.6, 0.95)
    arr = np.asarray(pageimg).astype(float) * paper_tint
    if scene == 'held' and rng.random() < 0.6:
        through = np.asarray(doc.convert('L').transpose(Image.FLIP_LEFT_RIGHT).transform((W, H), Image.PERSPECTIVE, coeffs, Image.BILINEAR, fillcolor=255)).astype(float)
        arr = arr * (0.75 + 0.25 * through / 255)[..., None]
    m = np.asarray(mask).astype(float)[..., None] / 255
    if scene == 'stack':
        for _ in range(rng.integers(1, 3)):
            shift = rng.uniform(-1, 1, 2) * [W * 0.12, H * 0.1] + [0, -rng.uniform(0, 0.15) * H]
            turn = rng.uniform(-0.12, 0.12)
            spin = np.array([[np.cos(turn), -np.sin(turn)], [np.sin(turn), np.cos(turn)]])
            centre = corners.mean(0)
            under = (corners - centre) @ spin.T + centre + shift
            sheet = Image.new('L', doc.size, int(rng.uniform(225, 250)))
            sd = ImageDraw.Draw(sheet)
            for _ in range(rng.integers(5, 25)):
                y0 = rng.uniform(0, doc.height)
                sd.line((rng.uniform(0, doc.width / 2), y0, rng.uniform(doc.width / 2, doc.width), y0 + rng.uniform(-30, 30)), fill=int(rng.uniform(40, 120)), width=int(rng.uniform(3, 8)))
            uc = perspective(under, src)
            ui = np.asarray(sheet.convert('RGB').transform((W, H), Image.PERSPECTIVE, uc, Image.BILINEAR, fillcolor=0)).astype(float) * rng.uniform(0.85, 1.0, 3)
            um = np.asarray(Image.new('L', doc.size, 255).transform((W, H), Image.PERSPECTIVE, uc, Image.BILINEAR, fillcolor=0)).astype(float)[..., None] / 255
            background = ui * um * 0.97 + background * (1 - um)
    out = arr * m + background * (1 - m)
    if scene == 'held':
        hand = Image.new('L', (W, H), 0)
        hd = ImageDraw.Draw(hand)
        side = rng.integers(0, 4)
        a, b = (corners[side], corners[(side + 1) % 4])
        t = rng.uniform(0.2, 0.8)
        px, py = a + (b - a) * t
        r = rng.uniform(90, 200)
        hd.ellipse((px - r, py - r * 1.3, px + r, py + r * 1.3), fill=255)
        hm = np.asarray(hand.filter(ImageFilter.GaussianBlur(8))).astype(float)[..., None] / 255
        skin = np.array([rng.uniform(90, 200), rng.uniform(60, 150), rng.uniform(40, 120)])
        out = out * (1 - hm) + skin * hm
    x = np.linspace(-1, 1, W)[None, :]
    yy = np.linspace(-1, 1, H)[:, None]
    slope = 1 - 0.35 * difficulty * (x * rng.uniform(-1, 1) + yy * rng.uniform(-1, 1))
    sx, sy = rng.uniform(-0.6, 0.6, 2)
    shadow = 1 - 0.45 * difficulty * np.exp(-(((x - sx) / 0.35) ** 2 + ((yy - sy) / 0.5) ** 2))
    dark = 1 - 0.45 * difficulty * rng.random()
    out = out * (slope * shadow * dark)[..., None]
    img = Image.fromarray(np.clip(out, 0, 255).astype(np.uint8))
    blur = difficulty * rng.uniform(0.5, 3.5)
    if blur > 0.3:
        img = img.filter(ImageFilter.GaussianBlur(blur))
    arr = np.asarray(img).astype(float)
    arr += rng.normal(0, 3 + 14 * difficulty, arr.shape)
    img = Image.fromarray(np.clip(arr, 0, 255).astype(np.uint8))
    small = 1 - 0.45 * difficulty * rng.random()
    if small < 0.97:
        size = (int(W * small), int(H * small))
        img = img.resize(size, Image.BILINEAR)
        corners = corners * small
    quality = int(90 - 55 * difficulty * rng.random())
    buffer = io.BytesIO()
    img.save(buffer, 'JPEG', quality=quality)
    img = Image.open(io.BytesIO(buffer.getvalue())).convert('RGB')
    return (img, corners)

def perspective(dst, src):
    rows = []
    rhs = []
    for (x, y), (u, v) in zip(dst, src):
        rows.append([x, y, 1, 0, 0, 0, -u * x, -u * y])
        rows.append([0, 0, 0, x, y, 1, -v * x, -v * y])
        rhs += [u, v]
    return np.linalg.solve(np.array(rows), np.array(rhs)).tolist()

def main():
    out = sys.argv[1]
    count = int(sys.argv[2]) if len(sys.argv) > 2 else 60
    dressed = len(sys.argv) > 3 and sys.argv[3] == 'dressed'
    os.makedirs(out, exist_ok=True)
    rng = np.random.default_rng(20260927 if dressed else 20260926)
    truth = []
    for n in range(count):
        language = 'lao' if n % 3 == 0 else 'eng'
        difficulty = n / max(count - 1, 1)
        doc, text = page(rng, language)
        if dressed:
            doc = dress(rng, doc)
        img, corners = photograph(rng, doc, difficulty, busy=dressed)
        name = f'{n:03d}'
        img.save(os.path.join(out, name + '.ppm'))
        with open(os.path.join(out, name + '.txt'), 'w') as f:
            f.write(language + '\n' + text + '\n')
        flat = ' '.join((f'{v:.1f}' for v in corners.reshape(-1)))
        truth.append(f'{name} {flat} {difficulty:.2f}')
    with open(os.path.join(out, 'truth.txt'), 'w') as f:
        f.write('\n'.join(truth) + '\n')
    print(f'{count} pictures in {out}')
if __name__ == '__main__':
    main()
