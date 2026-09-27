#!/usr/bin/env python3
import os
import sys
from multiprocessing import Pool
import numpy as np
from PIL import Image, ImageDraw
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import make
W, H = (192, 256)
DRESSED = float(os.environ.get('DRESSED', '0.65'))
SCENES = [('held', 0.2), ('tiles', 0.2), ('stack', 0.15), ('wide', 0.15)]

def sample(args):
    seed, index = args
    rng = np.random.default_rng([seed, index])
    language = 'lao' if rng.random() < 0.35 else 'eng'
    fill = 1.0 if rng.random() < 0.5 else 0.0 if rng.random() < 0.15 else rng.uniform(0.1, 0.7)
    doc, _ = make.page(rng, language, fill)
    if rng.random() < DRESSED:
        doc = make.dress(rng, doc)
    difficulty = rng.random()
    pick = rng.random()
    scene = None
    for name, share in SCENES:
        if pick < share:
            scene = name
            break
        pick -= share
    img, corners = make.photograph(rng, doc, difficulty, busy=rng.random() < DRESSED, scene=scene)
    sx, sy = (W / img.width, H / img.height)
    small = img.resize((W, H), Image.BILINEAR)
    mask = Image.new('L', (W, H), 0)
    ImageDraw.Draw(mask).polygon([(x * sx, y * sy) for x, y in corners], fill=1)
    return (np.asarray(small, dtype=np.uint8), np.asarray(mask, dtype=np.uint8))

def main():
    out, count = (sys.argv[1], int(sys.argv[2]))
    seed = int(sys.argv[3]) if len(sys.argv) > 3 else 7
    with Pool(10) as pool:
        rows = pool.map(sample, [(seed, i) for i in range(count)], chunksize=8)
    pictures = np.stack([r[0] for r in rows])
    masks = np.stack([r[1] for r in rows])
    np.savez_compressed(out, pictures=pictures, masks=masks)
    print(out, pictures.shape, masks.mean())
if __name__ == '__main__':
    main()
