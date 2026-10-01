<div align="center">

<img src=".github/logo.png" width="110" alt="">

# PanPDF for Android

The Android app of [PanPDF](https://github.com/panXDgaming/panpdf.rs).

### [⬇ Download PanPDF.apk](https://github.com/panXDgaming/panpdf.ad/releases/latest/download/PanPDF.apk)

Free &middot; open source &middot; no account &middot; Android 8 or newer

<img src="img/editor-phone.webp" width="260" alt="Editing a PDF in PanPDF on a phone">

</div>

## Install

1. Open the link above on your phone and tap the downloaded file.
2. If Android asks, allow your browser to install apps.
3. Tap **Install**.

Not on Google Play yet. Rather not install? Use it in the browser at
[panpdf.org](https://panpdf.org).

## The assistant

Tap the assistant button in the top bar to ask a model about the page you are
reading, or to have it edit, stamp, convert and read your document for you.
On a phone it covers the page; **Close** (or the back button) puts it away.

- **API key.** Choose OpenAI, Claude or Gemini, paste your key and tap
  **Connect**. The key is locked and kept in the app's own folder on the
  phone (`files/panpdf/ai-key`, readable by PanPDF alone); it is sent only to
  the provider you chose.
- **Models on your own network.** The app connects over HTTPS. Plain HTTP is
  allowed only for `localhost`, `127.0.0.1` and `10.0.2.2` (the emulator's name
  for its computer), so an Ollama or LM Studio server on the phone itself, or
  on the emulator's host, works without a key. Anything else must use HTTPS.
- Answers arrive whole rather than word by word: the phone's network is used
  through the system's HTTP stack, not `curl`.

## Build it yourself

Needs Rust (pinned in `engine/rust-toolchain.toml`) with the
`aarch64-linux-android` target, and under `~/Android/Sdk` the NDK 27.3,
build-tools 35 and platform 35. The converters come from
[pdf_tool](https://github.com/panXDgaming/pdf_tool), cloned beside
this folder.

```sh
git clone https://github.com/panXDgaming/panpdf.ad
git clone https://github.com/panXDgaming/pdf_tool
cd panpdf.ad
python3 engine/fonts/vendor.py              # the fonts the app carries
platforms/android/ocr-build.sh arm64        # Tesseract
platforms/android/tools-build.sh arm64      # the converters
platforms/android/build.sh arm64            # -> platforms/android/out/panpdf.apk
```

| Folder | What is in it |
| --- | --- |
| `platforms/android/` | the activity, the resources and the build scripts |
| `engine/crates/pdf-android` | the native library the activity loads |
| `engine/crates/pdf-window` | the editor's window, the same on every platform |
| `engine/crates/pdf-scan` | finding the page in a photo and flattening it |
| `engine/crates/pdf-agent` | the assistant's conversation, tools and, on the phone, its network |
| `engine/convert/` | the converters the assistant and the desktop's Tools room run in-process |
| `engine/` | the rest of PanPDF's engine |

## Licence

GNU Affero General Public License 3.0, as PanPDF. The parts made by others
are listed in the app under **About PanPDF and licences**.
