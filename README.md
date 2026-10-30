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
lacks (`-max-pages`, `-max-pixels`, `-max-memory`, `-max-time`,
`-png-compress`, `-strict`).

Independent project; not affiliated with Google, PDFium, pdfium-binaries, or pdfium-render.

## Status

Early. 0.1.0 was written in a morning, largely by an AI agent under human
direction and review, then checked against 1000 Common Crawl PDFs (see
[docs/pdftoppm-comparison.md](docs/pdftoppm-comparison.md)). Expect rough
edges; bug reports with a sample PDF are welcome.

## Options

```
pdfiumtoppm [options] <PDF-file> [<image-root>]
  -f <int>            first page
  -l <int>            last page
  -r <fp>             DPI (default 150)
  -scale-to <int>     scale the long edge to this many pixels
  -max-pages <int>    render at most this many pages (after -f/-l)
  -max-pixels <int>   downscale any page whose width*height exceeds this
  -max-memory <int>   memory limit in MiB; exit 4 if any page hits it
                      (Linux/Windows only; default 4096 or half of RAM,
                      whichever is lower; 0 = none)
  -max-time <fp>      wall-clock limit in seconds for the whole run; exit 5
  -png                write PNG (default binary PPM/PGM)
  -png-compress <int> PNG zlib level 0-9 (default 1; pdftoppm uses 6)
  -jpeg               write JPEG
  -jpegopt <string>   quality=0-100 (default 75), progressive=y|n,
                      optimize=y|n; comma-separated
  -gray               grayscale
  -cropbox            crop box rather than media box (always on; accepted)
  -singlefile         write only the first page, as <image-root>.<ext>
  -manifest <path>    write one JSON line per page (see below)
  -strict             exit 99 if any page failed to render
  -opw <string>       owner password
  -upw <string>       user password
  -pdfium <path>      directory containing the pdfium library
  -v                  print version
  -h                  print usage
```

Output: `<image-root>-<N>.<ext>`, where `N` is the PDF page number
zero-padded to the width of the document's page count, as in `pdftoppm`:
`-f 101 -l 200` on a 250-page PDF writes `out-101` through `out-200`, and
`-f 3` on a 12-page PDF starts at `out-03`.
With `-singlefile`, only the first page of the range is written, as
`<image-root>.<ext>`. Without `<image-root>`, images go to stdout one after
another, as in `pdftoppm` (a root of `-` is a file prefix, not stdout). A page
that fails is missing from the stream; use `-strict` to make that an error.
Stdout is not atomic: a crash can leave a partial image in the stream.
Each page is written to `<file>.part` and renamed into place when complete, so
an output file that exists is never truncated, even after a crash or a full
disk. PNGs carry a pHYs chunk and JPEGs a JFIF density with the effective DPI, as
`pdftoppm`'s do; OCR
engines size text from it (tesseract mis-segments tables without it).
Exit codes as `pdftoppm`: 0 ok, 1 could not open PDF, 2 could not write, 99
other (including an empty page range); plus, ours, 4 when `-max-memory` was
hit and 5 when `-max-time` was.

`-manifest <path>` writes one JSON object per line for each page attempted,
appended and flushed as the page finishes, so a crash or `-max-time` still
leaves every earlier record:

```
{"page":1,"status":"ok","file":"out-1.png","width":1275,"height":1650,"dpi":150.00,"downscaled":false,"bytes":10190,"ms":2}
{"page":2,"status":"failed","ms":0,"error":"degenerate page size 0x100 pt"}
{"page":3,"status":"timeout"}
```

`status` is `ok`, `failed`, `memory` (over `-max-memory`), or `timeout` (the
page in progress when `-max-time` hit). `dpi` is the effective resolution after
`-scale-to`/`-max-pixels`; `downscaled` says `-max-pixels` shrank the page.
`file` is absent for stdout. `bytes` and the size fields appear only for `ok`;
`ms` covers render and encode. Fields may be added; existing ones keep their
meaning.

`-max-time` bounds the whole run, from start to the last page. PDFium cannot
abort a page mid-render, so when the limit hits the process ends at once:
pages already written are complete, the page in progress is removed, and
stderr names it. Unlike an external `timeout`, it works the same on Windows
and leaves no `.part` behind.

`-max-pages`, `-max-pixels`, `-max-memory`, and `-max-time` are additions for untrusted
input. `-scale-to` matches `pdftoppm`, including enlarging; `-max-pixels`
only ever downscales. `-max-memory` sets `RLIMIT_AS` on Linux and a Job Object
commit limit on Windows before the pdfium library is loaded. It is not
supported elsewhere (macOS rejects `RLIMIT_AS`); there the option is off by
default and an explicit nonzero value is rejected. On supported
platforms it defaults to 4096 MiB or half of physical RAM, whichever is
lower; `-max-memory 0` turns it off. A page that would not fit is skipped with a message and the run
exits 4 after the remaining pages. If the process instead dies mid-page from
a fatal signal (an allocation failure inside PDFium ends that way), the exit
code is a guess: 4 when the address space was within a third of the limit,
99 when it was well under, since a crash that far from the limit is more
likely a PDFium bug than memory. Either way the message names the signal,
and a `<file>.part` for the page being written may be left behind. Rust
prints `fatal runtime error: Rust cannot catch foreign exceptions, aborting`
just before that message when PDFium's allocation failure surfaces as a C++
exception; that line is expected. Without `-max-memory` no signal is caught;
a crash is a crash. The crash-code guess is Linux-only: on Windows a page over
the limit is still skipped with exit 4, but if PDFium itself dies mid-page the
process exits with the OS status instead. The bitmap alone needs about 3 bytes
per output pixel on top of a ~64 MiB baseline (a 4096x4096 render, ~48 MiB
more); that is a floor, not a
budget. Page content can demand any amount regardless of output size (a
5 KB PDF has been seen to take 18 GB and the machine with it), so the
default limit exists for untrusted input; raise it if a legitimate page
needs more. Without a limit (`-max-memory 0`, or always on macOS) the only
backstop is the OS: Linux's out-of-memory killer ends the process; macOS has
no such killer and compresses and swaps until the whole system stalls.

A page that fails to render is reported and skipped; exit stays 0 unless no
page rendered (99). With `-strict` any failed page exits 99, after the
remaining pages are rendered.

`-jpeg` matches `pdftoppm`'s `-jpeg`/`-jpegopt` (libjpeg defaults: quality 75,
4:2:0 chroma subsampling at every quality). Differences: `-gray -jpeg` writes
a one-component grayscale JPEG where `pdftoppm` writes three components, and
`-jpeg` with `-png` or an invalid `-jpegopt` is a usage error (exit 99) rather
than `-png` winning or a message with exit 0. JPEG is limited to 65535 pixels
per side; a larger page fails like any other bad page.

Not supported: `-x/-y/-W/-H`, `-jpegcmyk`, `-tiff`, `-mono`, `-aa*`,
`-hide-annotations`, `-sep`, PDF from stdin.

Pages are always sized and rendered from the crop box (PDFium), as
`pdftoppm -cropbox` does; `-cropbox` is accepted so `pdftoppm` command lines
work unchanged. Without it, `pdftoppm` uses the media box, so output differs
for pages whose crop box is smaller.

## Install

Release archives contain `pdfiumtoppm` and a matching pdfium library; keep
them together:

```sh
tar xzf pdfiumtoppm-v0.3.0-linux-x64.tar.gz
sudo mv pdfiumtoppm-v0.3.0-linux-x64 /opt/pdfiumtoppm
sudo ln -s /opt/pdfiumtoppm/pdfiumtoppm /usr/local/bin/pdfiumtoppm
```

Linux x86_64 and arm64, macOS arm64, and Windows x64. The macOS zip holds
`pdfiumtoppm` with `libpdfium.dylib` beside it. The Windows zip holds
`pdfiumtoppm.exe` with `pdfium.dll` beside it; extract either archive anywhere
and keep its two binaries together. The macOS binaries are not notarized:
after a browser download, clear the quarantine flag before the first run, or
Gatekeeper blocks both files:

```sh
xattr -dr com.apple.quarantine pdfiumtoppm-v0.3.0-mac-arm64
```

### macOS and untrusted PDFs

The native macOS binary cannot enforce `-max-memory`: macOS rejects finite
`RLIMIT_AS` values, and its other documented process policies do not provide a
dependable hard memory cap. A small PDF can make PDFium allocate many
gigabytes, independently of output dimensions. `-max-pixels` and a timeout are
useful but are not substitutes for a memory boundary.

Do not render untrusted or bulk-corpus PDFs with the native macOS binary. Run
the Linux arm64 release in a local Docker container so Linux `RLIMIT_AS` and a
cgroup protect the Mac. For example, download and verify the Linux arm64
release, extract it into an otherwise empty directory, and add this
`Dockerfile` beside the extracted directory:

```dockerfile
FROM ubuntu:24.04
COPY pdfiumtoppm-v0.3.0-linux-arm64 /opt/pdfiumtoppm
ENTRYPOINT ["/opt/pdfiumtoppm/pdfiumtoppm"]
```

Build and run it locally:

```sh
docker build --platform linux/arm64 -t pdfiumtoppm:0.3.0 .
mkdir -p input output
docker run --rm --platform linux/arm64 \
  --memory=3g --memory-swap=3g --pids-limit=64 \
  --network=none --read-only --cap-drop=ALL \
  --security-opt=no-new-privileges \
  -v "$PWD/input:/input:ro" -v "$PWD/output:/output" \
  pdfiumtoppm:0.3.0 \
  -max-memory 2048 -max-pixels 12000000 -png \
  /input/document.pdf /output/page
```

`-max-memory 2048` is the primary, per-process Linux address-space limit. The
3 GiB cgroup is a separate hard backstop if PDFium crashes through that path.
Setting `--memory-swap` equal to `--memory` prevents additional swap allowance.
The input mount is read-only, output is the only writable host mount, and the
container has no network or Linux capabilities. Docker Desktop's local Linux
VM must have at least 3 GiB available. Adjust both limits deliberately for
legitimate large documents; never remove both limits for untrusted input.

## libpdfium

Dynamically loaded, not embedded (`libpdfium.so` on Linux,
`libpdfium.dylib` on macOS, `pdfium.dll` on Windows). Search order:
`-pdfium <dir>` (must bind), then `$PDFIUM_PATH`, the executable's directory,
the system library path. On failure the error lists every location tried.

```sh
mkdir -p pdfium && curl -sL https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/8125/pdfium-linux-x64.tgz | tar xz -C pdfium
PDFIUM_PATH=pdfium/lib pdfiumtoppm -png -r 300 -scale-to 4096 -gray in.pdf out
```

Tested and pinned (CI, release archives) to
[pdfium-binaries](https://github.com/bblanchon/pdfium-binaries/releases)
`chromium/8125`.

## Build

```sh
cargo build --release                  # target/release/pdfiumtoppm
cargo test --release                   # sizing and page-range logic; no libpdfium needed
PDFIUM_PATH=pdfium/lib tests/smoke.sh  # end to end
```

Tags `v*` publish `pdfiumtoppm-<tag>-linux-{x64,arm64}.tar.gz` and
`pdfiumtoppm-<tag>-{mac-arm64,win-x64}.zip`.

## Comparing with pdftoppm

`tests/compare.py <pdf-dir>` runs both tools over a corpus and reports
differences, timings, and whether a page scores better with its red and
blue channels swapped (the 0.1.0 color bug; mean pixel distance alone does
not catch it); results and caveats are in
[docs/pdftoppm-comparison.md](docs/pdftoppm-comparison.md). Rasterization is
~1.5-2x faster; end-to-end PDF-to-PNG is ~7-10x, almost entirely because
`pdftoppm` has no encoder-effort option (`-png-compress` here). For
tesseract, fast PNG beats PPM: raw PPM/PGM is ~10x the bytes and carries no
DPI, so tesseract must be told the resolution explicitly or it may
mis-segment.

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
installable. The [`image`](https://github.com/image-rs/image),
[`png`](https://github.com/image-rs/image-png), and
[`jpeg-encoder`](https://github.com/vstroebel/jpeg-encoder) crates do the
encoding; this software is based in part on the work of the Independent JPEG
Group. And [Poppler](https://poppler.freedesktop.org/)'s `pdftoppm` set the
interface and behavior this tool imitates; it has served as the workhorse of
countless pipelines for two decades and is the yardstick used throughout this
README. Thank you to all of them.

## License

Apache-2.0. Release archives also ship the pdfium library (BSD-3-Clause plus
its bundled third-party licenses, under `licenses/`).
