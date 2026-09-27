import json, os, re, subprocess
here = os.path.dirname(os.path.abspath(__file__))
web = os.path.dirname(os.path.dirname(here))
out = os.path.join(web, 'engine/crates/pdf-window/licences/android.txt')

def converters():
    for name in ('pdf_tool', 'panpdf-convert'):
        path = os.path.join(os.path.dirname(web), name)
        if os.path.isdir(path):
            return path
    raise SystemExit('the converters are not beside this checkout: clone pdf_tool next to it')
PROGRAMS = [(os.path.join(web, 'engine'), 'pdf-android'), (converters(), 'panpdf-tools')]
TARGET = 'aarch64-linux-android'
PREFER = ['MIT', 'Apache-2.0', 'Zlib', 'ISC', 'BSD-2-Clause', 'BSD-3-Clause', 'Unicode-3.0', 'CC0-1.0', 'Unlicense', '0BSD']

def shipped(workspace, root_name):
    meta = json.loads(subprocess.run(['cargo', 'metadata', '--format-version', '1', '--filter-platform', TARGET], cwd=workspace, capture_output=True, text=True, check=True).stdout)
    packages = {p['id']: p for p in meta['packages']}
    nodes = {n['id']: n for n in meta['resolve']['nodes']}
    root = next((p['id'] for p in meta['packages'] if p['name'] == root_name))
    seen, stack = (set(), [root])
    while stack:
        i = stack.pop()
        if i in seen:
            continue
        seen.add(i)
        for dep in nodes[i]['deps']:
            if any((kind['kind'] is None for kind in dep['dep_kinds'])):
                stack.append(dep['pkg'])
    return [packages[i] for i in seen if packages[i]['source'] and 'panXDgaming/panpdf.rs' not in packages[i]['source']]

def choose(expression):
    expression = expression.replace('/', ' OR ')
    parts = [p.strip(' ()') for p in re.split('\\bAND\\b', expression)]
    taken = []
    for part in parts:
        options = [o.strip(' ()') for o in re.split('\\bOR\\b', part)]
        taken.append(min(options, key=lambda o: PREFER.index(o) if o in PREFER else len(PREFER)))
    return taken

def licence_files(package):
    folder = os.path.dirname(package['manifest_path'])
    found = []
    for sub in ('', 'fonts'):
        where = os.path.join(folder, sub)
        if not os.path.isdir(where):
            continue
        for n in sorted(os.listdir(where)):
            pattern = '(?i)(licen[cs]e|copying|notice|unlicense)' if not sub else '(?i).*(licen[cs]e|ofl|ufl).*\\.txt$|hack-regular\\.txt$'
            if re.match(pattern, n) and os.path.isfile(os.path.join(where, n)):
                found.append((os.path.join(sub, n) if sub else n, os.path.join(where, n)))
    return found

def copyright_of(package):
    for name, path in licence_files(package):
        try:
            text = open(path, encoding='utf-8', errors='replace').read()
        except OSError:
            continue
        for line in text.splitlines():
            line = line.strip()
            if re.match('(?i)copyright\\s*(\\(c\\)|©|\\d)', line) and '[yyyy]' not in line and ('{yyyy}' not in line):
                return line
    authors = [re.sub('\\s*<[^>]*>', '', a) for a in package.get('authors') or []]
    return 'Copyright the authors' + (': ' + ', '.join(authors) if authors else f' of {package['name']}')
MIT = 'Permission is hereby granted, free of charge, to any person obtaining a copy\nof this software and associated documentation files (the "Software"), to deal\nin the Software without restriction, including without limitation the rights\nto use, copy, modify, merge, publish, distribute, sublicense, and/or sell\ncopies of the Software, and to permit persons to whom the Software is\nfurnished to do so, subject to the following conditions:\n\nThe above copyright notice and this permission notice shall be included in all\ncopies or substantial portions of the Software.\n\nTHE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR\nIMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,\nFITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE\nAUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER\nLIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,\nOUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE\nSOFTWARE.'

def main():
    libraries = {}
    for workspace, root in PROGRAMS:
        for package in shipped(workspace, root):
            libraries[package['name'], package['version']] = package
    apache = open(os.path.join(web, 'engine/crates/pdf-scan/models/docquad-LICENSE.txt')).read().strip()
    leptonica = open(os.path.expanduser('~/Android/src/leptonica-1.85.0/leptonica-license.txt')).read().strip()
    lines = []
    w = lines.append
    w('PanPDF for Android is free software under the GNU Affero General Public')
    w("License, version 3. The program's source is at")
    w('https://github.com/panXDgaming/panpdf.rs.')
    w('')
    w('It carries parts made by others, each under its own licence:')
    w('')
    w('== Tesseract OCR 5.5.1')
    w('https://github.com/tesseract-ocr/tesseract -- Apache License 2.0 (full text below).')
    w('Its language models (tessdata_fast), downloaded when a language is chosen,')
    w('are under the Apache License 2.0 too.')
    w('')
    w('== Leptonica 1.85.0')
    w('http://www.leptonica.org -- used by Tesseract, under this licence:')
    w('')
    w(leptonica)
    w('')
    w('== DocQuadNet, the page-corner model')
    w('From MakeACopy by egdels, https://github.com/egdels/makeacopy, under the')
    w('Apache License 2.0 (full text below). Changed only in form: read from ONNX')
    w("Runtime's .ort format, its fused convolutions written out as their parts,")
    w('and its weights stored at half precision.')
    w('')
    w('== Fonts')
    w('Noto (Latin, Thai, Lao, CJK and others), DejaVu, Liberation and Saysettha OT,')
    w('under the SIL Open Font License 1.1 and their own licences; each licence')
    w('file is in the app beside the fonts (assets/fonts/packaged).')
    w('')
    w(f'== Rust libraries ({len(libraries)})')
    w('')
    own_files = []
    for (name, version), package in sorted(libraries.items()):
        taken = choose(package.get('license') or 'unknown')
        w(f'{name} {version} -- {' AND '.join(taken)}')
        w(f'  {copyright_of(package)}')
        if any((t not in ('MIT', 'Apache-2.0') for t in taken)):
            own_files.append(package)
    w('')
    w('== The MIT License, as the libraries above marked MIT give it')
    w('')
    w(MIT)
    w('')
    for package in own_files:
        w(f'== {package['name']} {package['version']}: its own licence files')
        files = licence_files(package)
        if not files:
            w('(none in the package: ' + ' AND '.join(choose(package['license'])) + ', which asks for no notice)')
            w('')
        for filename, path in files:
            w(f'-- {filename}')
            w(open(path, encoding='utf-8', errors='replace').read().strip())
            w('')
    w('== Apache License 2.0')
    w('')
    w(apache)
    os.makedirs(os.path.dirname(out), exist_ok=True)
    open(out, 'w').write('\n'.join(lines) + '\n')
    print(out, len(libraries), 'libraries,', os.path.getsize(out), 'bytes')
main()
