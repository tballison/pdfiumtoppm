<!--
Copyright 2026 Tim Allison

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
-->

# pdfiumtoppm

A PDF page renderer using [PDFium](https://pdfium.googlesource.com/pdfium/)
via [pdfium-render](https://crates.io/crates/pdfium-render) that speaks a
subset of Poppler's `pdftoppm` command line. Unsupported flags are rejected,
not ignored; `pdftoppm` remains the reference for behavior here. It does not
promise to track Poppler's development, and it adds a few flags `pdftoppm`
lacks (`-max-pages`, `-max-pixels`, `-max-memory`, `-png-compress`).

Independent project; not affiliated with Google, PDFium, pdfium-binaries, or pdfium-render.

## Status

Early (0.1.0). Written in a morning, largely by an AI agent under human
direction and review, then checked against 100 Common Crawl PDFs (see
[docs/pdftoppm-comparison.md](docs/pdftoppm-comparison.md)). Expect rough
edges; bug reports with a sample PDF are welcome.

## Options

```
pdfiumtoppm [options] <PDF-file> <image-root>
  -f <int>            first page
  -l <int>            last page
  -r <fp>             DPI (default 150)
  -scale-to <int>     scale the long edge to this many pixels
  -max-pages <int>    render at most this many pages (after -f/-l)
  -max-pixels <int>   downscale any page whose width*height exceeds this
  -max-memory <int>   address-space limit in MiB; exit 4 if any page hits it
  -png                write PNG (default binary PPM/PGM)
  -png-compress <int> PNG zlib level 0-9 (default 1; pdftoppm uses 6)
  -gray               grayscale
  -opw <string>       owner password
  -upw <string>       user password
  -pdfium <path>      directory containing libpdfium.so
```

Output: `<image-root>-<N>.<ext>`, `N` zero-padded to the page-count width.
Exit codes as `pdftoppm`: 0 ok, 1 could not open PDF, 2 could not write, 99
other (including an empty page range); plus 4, ours, when `-max-memory` was
hit.

`-max-pages`, `-max-pixels`, and `-max-memory` are additions for untrusted
input. `-scale-to` matches `pdftoppm`, including enlarging; `-max-pixels`
only ever downscales. `-max-memory` (Unix only) sets `RLIMIT_AS` before `libpdfium` is
loaded. A page that would not fit is skipped with a message and the run
exits 4 after the remaining pages; if PDFium itself runs out of memory
mid-page the process still exits 4, but any output file being written at
that moment may be truncated. Budget about 8 bytes per output pixel on top
of a ~64 MiB baseline (a 4096x4096 render needs ~130 MiB more).

A page that fails to render is reported and skipped; exit stays 0 unless no
page rendered (99).

Not supported: `-cropbox`, `-x/-y/-W/-H`, `-singlefile`, `-jpeg`, `-tiff`,
`-mono`, `-aa*`, `-hide-annotations`, stdout. Pages are sized from the crop
box (PDFium); `pdftoppm` defaults to the media box.

## Install

Release tarballs contain `pdfiumtoppm` and a matching `libpdfium.so`; keep
them together:

```sh
tar xzf pdfiumtoppm-v0.1.0-linux-x64.tar.gz
sudo mv pdfiumtoppm-v0.1.0-linux-x64 /opt/pdfiumtoppm
sudo ln -s /opt/pdfiumtoppm/pdfiumtoppm /usr/local/bin/pdfiumtoppm
```

Linux x86_64 and arm64.

## libpdfium

Dynamically loaded, not embedded. Search order: `-pdfium <dir>` (must bind),
then `$PDFIUM_PATH`, the executable's directory, the system library path. On
failure the error lists every location tried.

```sh
mkdir -p pdfium && curl -sL https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/8021/pdfium-linux-x64.tgz | tar xz -C pdfium
PDFIUM_PATH=pdfium/lib pdfiumtoppm -png -r 300 -scale-to 4096 -gray in.pdf out
```

Tested and pinned (CI, release tarball) to
[pdfium-binaries](https://github.com/bblanchon/pdfium-binaries/releases)
`chromium/8021`.

## Build

```sh
cargo build --release                  # target/release/pdfiumtoppm
PDFIUM_PATH=pdfium/lib tests/smoke.sh
```

Tags `v*` publish `pdfiumtoppm-<tag>-linux-{x64,arm64}.tar.gz`.

## Comparing with pdftoppm

`tests/compare.py <pdf-dir>` runs both tools over a corpus and reports
differences and timings; results and caveats are in
[docs/pdftoppm-comparison.md](docs/pdftoppm-comparison.md). Rendering is
~2x faster with a shorter tail; PNG timing differences are mostly encoder
settings (`-png-compress`). For tesseract, write PPM.

## Credits

This is a thin shell; the real work belongs to others.
[PDFium](https://pdfium.googlesource.com/pdfium/) is Google's PDF engine from
Chromium. Alastair Carey's
[pdfium-render](https://github.com/ajrcarey/pdfium-render) makes it usable
from Rust, and Benoit Blanchon's
[pdfium-binaries](https://github.com/bblanchon/pdfium-binaries) makes it
installable. The [`image`](https://github.com/image-rs/image) crate does the
encoding. And [Poppler](https://poppler.freedesktop.org/)'s `pdftoppm` set the
interface and behavior this tool imitates; it has served as the workhorse of
countless pipelines for two decades and is the yardstick used throughout this
README. Thank you to all of them.

## License

Apache-2.0. Release tarballs also ship `libpdfium.so` (BSD-3-Clause plus its
bundled third-party licenses, under `licenses/`).
