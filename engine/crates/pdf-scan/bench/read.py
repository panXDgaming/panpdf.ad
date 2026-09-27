#!/usr/bin/env python3
import os
import subprocess
import sys

def distance(a, b):
    previous = list(range(len(b) + 1))
    for i, x in enumerate(a, 1):
        current = [i]
        for j, y in enumerate(b, 1):
            current.append(min(previous[j] + 1, current[j - 1] + 1, previous[j - 1] + (x != y)))
        previous = current
    return previous[-1]

def squeeze(text):
    return ''.join(text.split())

def read(tesseract, tessdata, path, language):
    done = subprocess.run([tesseract, path, 'stdout', '-l', language, '--tessdata-dir', tessdata, '--psm', '4'], capture_output=True, text=True, timeout=300)
    return done.stdout

def main():
    folder, tesseract, tessdata = sys.argv[1:4]
    truths = {}
    for line in open(os.path.join(folder, 'truth.txt')):
        fields = line.split()
        truths[fields[0]] = float(fields[9])
    bands = {}
    for name, difficulty in sorted(truths.items()):
        with open(os.path.join(folder, name + '.txt')) as f:
            language = f.readline().strip()
            truth = squeeze(f.read())
        errors = {}
        for kind, suffix in (('grey', '-grey.pgm'), ('colour', '-colour.ppm')):
            got = squeeze(read(tesseract, tessdata, os.path.join(folder, name + suffix), language))
            errors[kind] = distance(got, truth) / max(len(truth), 1)
        band = min(int(difficulty * 4), 3)
        bands.setdefault(band, []).append(errors)
        print(f'{name} {language} difficulty {difficulty:.2f} grey {errors['grey']:.1%} colour {errors['colour']:.1%}', flush=True)
    for band in sorted(bands):
        rows = bands[band]
        grey = sum((r['grey'] for r in rows)) / len(rows)
        colour = sum((r['colour'] for r in rows)) / len(rows)
        print(f'difficulty {band / 4:.2f}-{(band + 1) / 4:.2f}: grey {grey:.1%} colour {colour:.1%} ({len(rows)} pages)')
if __name__ == '__main__':
    main()
