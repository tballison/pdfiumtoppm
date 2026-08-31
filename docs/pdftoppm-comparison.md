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

First 100 files of `0000.zip`, first 3 pages, Poppler 24.02 vs pdfiumtoppm
0.1.1 (pdfium `chromium/8021`, pdfium-render 0.9.3, image 0.25.10), one
Linux x86_64 box: no exit-code or page-count differences; 3 files with the
1-pixel A4 size difference described below; no channel-swapped pages;
RGB similarity median 0.97 (72 DPI) to 0.99 (300 DPI), min 0.91 (all low
cases: non-embedded font substitution). Total wall time:

| flags         | pdfiumtoppm | pdftoppm | notes |
|---------------|-------------|----------|-------|
| `-r 72` PPM   |  4.6 s |   8.8 s | |
| `-r 72 -png`  |  5.4 s |  32.5 s | |
| `-r 300` PPM  | 23.2 s |  49.7 s | equal medians; worst file 1.6 s vs 13.2 s |
| `-r 300 -png` | 30.1 s | 293.2 s | |

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

## Whole-document comparison, 1,000 files

Same corpus source, files `0000000.pdf`-`0000999.pdf` of `0000.zip` (16,248
pages), every page of every document in one invocation per tool, gray
200 DPI, 4 concurrent workers, Poppler 24.02 vs pdfiumtoppm 0.1.2, one
Linux x86_64 laptop (i7-1260P). Both tools opened the same 996 files and
failed the same 4 (one encrypted, three truncated).

Raw PGM output (no encoder; byte-identical 58.9 GB from each tool) isolates
rasterization:

| pages/file | pdfiumtoppm | pdftoppm ms/page | ratio |
|---|---|---|---|
| 1 | 100 | 151 | 1.5x |
| 4-10 | 41 | 66 | 1.6x |
| 101+ | 26 | 54 | 2.1x |
| total | 546 s | 995 s | 1.8x |

As rasterizers the two are close - Poppler's is excellent. Re-running
against Poppler 26.08 built from source (same corpus, same machine, same
run for both tools) gives the same picture: rasterization ratio 1.65x
(1.4x single-page to 1.8x on 100+ pages), and 26.08 measured no faster
than 24.02 here. The end-to-end gap is PNG encoding: `pdftoppm` has no
compression-effort option (still true in 26.08), while
this tool defaults to fast (`-png-compress 1`). PNG output on the same
corpus: 762 s vs 7,446 s (with 11 large documents hitting a 120-second
per-document cap that neither tool hits in PGM mode); with
`-png-compress 6` for comparable effort, 931 s vs 6,932 s.

With matched encoder effort (`-png-compress 6`) against 26.08 in the same
run, PDF-to-PNG is 7.1x (1,089 s vs 7,712 s over the files both completed),
with the same handful of large documents hitting the per-document cap.

Per-page agreement between the two across all rendered pages: mean RGB
similarity 0.984 (same metric as above), identical page counts, no
channel-order or geometry differences beyond the 1-pixel rounding noted
above.
