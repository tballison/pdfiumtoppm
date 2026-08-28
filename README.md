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
not ignored; `pdftoppm` remains the reference for behavior here. This project does not
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
                      (default 4096 or half of RAM, whichever is lower; 0 = none)
  -png                write PNG (default binary PPM/PGM)
  -png-compress <int> PNG zlib level 0-9 (default 1; pdftoppm uses 6)
  -gray               grayscale
  -opw <string>       owner password
  -upw <string>       user password
  -pdfium <path>      directory containing libpdfium.so
  -v                  print version
  -h                  print usage
```

Output: `<image-root>-<N>.<ext>`, `N` zero-padded to the page-count width.
PNGs carry a pHYs chunk with the effective DPI, as `pdftoppm`'s do; OCR
engines size text from it (tesseract mis-segments tables without it).
Exit codes as `pdftoppm`: 0 ok, 1 could not open PDF, 2 could not write, 99
other (including an empty page range); plus 4, ours, when `-max-memory` was
hit.

`-max-pages`, `-max-pixels`, and `-max-memory` are additions for untrusted
input. `-scale-to` matches `pdftoppm`, including enlarging; `-max-pixels`
only ever downscales. `-max-memory` (Unix only) sets `RLIMIT_AS` before `libpdfium` is
loaded. Unlike `pdftoppm`, it is on by default: 4096 MiB or half of physical
RAM, whichever is lower; `-max-memory 0` turns it off. A page that would not fit is skipped with a message and the run
exits 4 after the remaining pages. If the process instead dies mid-page from
a fatal signal (an allocation failure inside PDFium ends that way), the exit
code is a guess: 4 when the address space was within a third of the limit,
99 when it was well under, since a crash that far from the limit is more
likely a PDFium bug than memory. Either way the message names the signal,
and any output file being written at that moment may be truncated. Rust
prints `fatal runtime error: Rust cannot catch foreign exceptions, aborting`
just before that message when PDFium's allocation failure surfaces as a C++
exception; that line is expected. Without `-max-memory` no signal is caught;
a crash is a crash. The bitmap alone needs about 8 bytes per output pixel on top of a ~64 MiB
baseline (a 4096x4096 render, ~130 MiB more); that is a floor, not a
budget. Page content can demand any amount regardless of output size (a
5 KB PDF has been seen to take 18 GB and the machine with it), so the
default limit exists for untrusted input; raise it if a legitimate page
needs more, and know that with `-max-memory 0` the kernel's out-of-memory
killer is the only backstop.

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
cargo test --release                   # sizing and page-range logic; no libpdfium needed
PDFIUM_PATH=pdfium/lib tests/smoke.sh  # end to end
```

Tags `v*` publish `pdfiumtoppm-<tag>-linux-{x64,arm64}.tar.gz`.

## Comparing with pdftoppm

`tests/compare.py <pdf-dir>` runs both tools over a corpus and reports
differences, timings, and whether a page scores better with its red and
blue channels swapped (the 0.1.0 color bug; mean pixel distance alone does
not catch it); results and caveats are in
[docs/pdftoppm-comparison.md](docs/pdftoppm-comparison.md). Rendering is
~2x faster with a shorter tail; PNG timing differences are mostly encoder
settings (`-png-compress`). For tesseract, write PPM.

## Related tools

Other ways to render with PDFium, each with its own strengths:

- [pdfium-cli](https://github.com/klippa-app/pdfium-cli) (Go, MIT): a
  broader PDF toolkit that renders to PNG/JPG and can run PDFium as
  WebAssembly, which gives real isolation for untrusted input.
- [pypdfium2](https://github.com/pypdfium2-team/pypdfium2) (Python): mature
  bindings with a `render` command and parallel page rendering.
- `pdfium_test`, PDFium's own test harness, writes PPM/PNG from the source tree.
- [Poppler](https://poppler.freedesktop.org/)'s `pdftoppm`: the original, with
  far more options than this tool supports.

This tool's only distinction is speaking `pdftoppm`'s command line with
`-max-*` limits, as one small binary.

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
