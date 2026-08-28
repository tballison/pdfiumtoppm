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
