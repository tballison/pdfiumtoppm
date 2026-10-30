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

# Changes

## Unreleased

- `-jpeg` and `-jpegopt quality=,progressive=,optimize=`, as `pdftoppm`.
- `-strict`: exit 99 if any page failed to render.
- PDFium renders 24-bit BGR into a buffer that is converted in place, instead
  of 32-bit BGRA plus two copies: peak memory per page drops from about 8 to
  3 bytes per pixel, and conversion is faster. Output can differ by 1 in a few
  anti-aliased pixels.
- `-max-time <seconds>`: wall-clock limit for the run; exit 5 when hit, after
  removing the page in progress.
- `-manifest <path>`: one JSON line per page with status, file, size,
  effective DPI, bytes, and time.
- `-cropbox` is accepted; it was already the behavior.
- `-singlefile`, and output to stdout when `<image-root>` is omitted, as
  `pdftoppm`.
- README states that output files are numbered by PDF page number.
- Pages are written to `<file>.part` and renamed when complete; a crash or
  write error no longer leaves a truncated page file. A write error in the
  final buffer flush or PNG end chunk, previously ignored, now exits 2.

## 0.2.1

- macOS arm64 release zip (`pdfiumtoppm` plus `libpdfium.dylib`).
- The pdfium library next to the executable is found through a symlink to
  it (macOS `current_exe()` does not resolve symlinks).
- CI and release smoke coverage on the GitHub-hosted macOS 15 arm64 runner.
- `-max-memory` is now Linux and Windows only: elsewhere it is disabled by
  default and rejected when explicitly enabled (on macOS
  `setrlimit(RLIMIT_AS)` returns `EINVAL`). Render untrusted
  PDFs in isolated subprocesses with an external memory limit on macOS.
- The README recommends a constrained local Linux arm64 Docker container for
  untrusted PDFs on macOS and provides a cgroup-backed example.
- pdfium-binaries `chromium/8125` (PDFium 158.0.8125.0).
- CI checks the declared minimum Rust version (1.88).
- pdfium-render 0.9.4, libc 0.2.191, and current compatible transitive
  dependencies.

## 0.2.0

- Windows x64 release zip (`pdfiumtoppm.exe` plus `pdfium.dll`).
- `-max-memory` now works on Windows via a Job Object commit limit, on by
  default as on Unix. The crash exit-code guess stays Unix-only: a page over
  the limit is skipped with exit 4, but a crash inside PDFium exits with the
  OS status.
- pdfium-binaries `chromium/8035` (PDFium 154.0.8035.0).

## 0.1.2

- PNG output carries a pHYs chunk with the effective DPI, as `pdftoppm`'s
  does. Without it tesseract guesses the resolution and mis-segments
  tables; a Tika OCR bake-off found a page going from perfect to junk for
  this reason alone. PNG is now encoded with the `png` crate directly.

## 0.1.1

- `-max-memory` now defaults to 4096 MiB or half of physical RAM, whichever
  is lower; `-max-memory 0` disables it. Previously there was no limit, and
  a 5 KB PDF could take the machine down with it.
- Fix red and blue swapped in color output (pdfium-render's default
  `FPDF_REVERSE_BYTE_ORDER` made the buffer RGBA while it was read as BGRA).
  Grayscale output was unaffected. `tests/compare.py` now compares in RGB,
  which is how the 0.1.0 corpus run missed this.

## 0.1.0

- Initial release: `pdftoppm`-compatible subset (`-f -l -r -scale-to -png
  -gray -opw -upw`), plus `-max-pages`, `-max-pixels`, `-max-memory`, `-png-compress`,
  `-pdfium`.
- Exit code 4 when `-max-memory` is hit. A fatal signal under `-max-memory`
  exits 4 if the address space was near the limit, else 99 (probably a
  PDFium bug); the message names the signal.
- Linux x64 and arm64 release tarballs.
- pdfium-binaries `chromium/8021` (PDFium 154.0.8021.0), pdfium-render 0.9.3,
  image 0.25.10.
