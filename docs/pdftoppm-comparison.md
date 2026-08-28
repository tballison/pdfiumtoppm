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

# Comparing with pdftoppm

`tests/compare.py <pdf-dir> [-n N] [--flags "..."]` runs both tools (pdftoppm
with `-cropbox`) and reports exit codes, page counts, dimensions, wall time,
and per-page similarity (needs Pillow + numpy). No corpus PDFs ship here.

A good source is Digital Corpora's
[CC-MAIN-2021-31-PDF-UNTRUNCATED](https://digitalcorpora.org/corpora/file-corpora/cc-main-2021-31-pdf-untruncated/):
~8 million PDFs from Common Crawl, packaged as 1000-file zips (~1.3 GB each)
under `zipfiles/<NNNN-NNNN>/<NNNN>.zip`, `0000.zip` through `7932.zip`:

```sh
curl -fLO https://digitalcorpora.s3.amazonaws.com/corpora/files/CC-MAIN-2021-31-PDF-UNTRUNCATED/zipfiles/0000-0999/0000.zip
unzip -q 0000.zip -d pdfs
PDFIUM_PATH=pdfium/lib tests/compare.py pdfs -n 100
```

Bucket listing: `https://digitalcorpora.s3.amazonaws.com/?prefix=corpora/files/CC-MAIN-2021-31-PDF-UNTRUNCATED/zipfiles/`.

First 100 files of `0000.zip`, first 3 pages, Poppler 24.02 vs pdfium
`chromium/8021` (pdfium-render 0.9.3, image 0.25.10), one Linux x86_64 box:
no exit-code or page-count differences; similarity median 0.97-0.99, min 0.91 (all low cases: non-embedded font substitution). Total
wall time:

| flags         | pdfiumtoppm | pdftoppm | notes |
|---------------|-------------|----------|-------|
| `-r 72` PPM   |  4.5 s |   8.5 s | |
| `-r 72 -png`  |  5.1 s |  33.3 s | |
| `-r 300` PPM  | 22.9 s |  49.2 s | equal medians; worst file 1.6 s vs 13.0 s |
| `-r 300 -png` | 26.0 s | 292.9 s | |

Rendering is ~2x faster with a shorter tail. The PNG rows mostly measure
encoding, where the two tools make different choices: `pdftoppm` spends time
to produce smaller files; this tool defaults to fast, larger output and
leaves the trade-off to `-png-compress`. One 300 DPI scanned page:

| encoder | time | size |
|---|---|---|
| `-png-compress 1` (default) | 0.6 s | 8.2 MB |
| `-png-compress 6` | 1.5 s | 5.6 MB |
| `-png-compress 9` | 4.4 s | 5.5 MB |
| pdftoppm | 10.0 s | 5.5 MB |

For tesseract, write PPM; it reads PPM/PGM directly.

Expect occasional 1-pixel size differences where `pt * dpi / 72` is an exact
integer (A4 at 300 DPI: 2482 vs 2483). `pdftoppm` also reports syntax
problems it finds in damaged files; this tool is silent unless a page fails.
