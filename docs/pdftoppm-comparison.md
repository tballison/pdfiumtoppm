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
and per-page similarity. Needs Pillow + numpy; SciPy adds the `ssim`
column. Per-page detail goes to `compare-pages.csv`; `--keep-below SIM`
copies both renders of any page scoring under SIM for inspection. No corpus
PDFs ship here.

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

## Current measurement (2026-09-01)

Experiment 3, numbered to follow the two in
[pdftoppm-comparison-2026-08.md](pdftoppm-comparison-2026-08.md).

Files `0000000.pdf`-`0000999.pdf` of `0000.zip`, first 3 pages (2,232
pages), color `-r 72 -png`, Poppler 24.02 vs pdfiumtoppm 0.2.0 (pdfium
`chromium/8035`, pdfium-render 0.9.3, image 0.25.10, png 0.18.1), one Linux
x86_64 laptop (i7-1260P). No exit-code, page-count, dimension, or
channel-swap differences on any of the 1,000 files.

Similarity is reported per page; a page is the unit that gets rendered.
The per-file view takes each file's worst page, which is both pessimistic
and sensitive to how many pages per file the sample happens to have, so the
two are not interchangeable:

| | per page (2,232) | per file (1,000) |
|---|---|---|
| median | 0.9765 | 0.9718 |
| mean | 0.9742 | - |
| min | 0.8509 | 0.8509 |
| below 0.90 | 4 pages | 3 files |

Wall time, whole run: 55.8 s vs 302.4 s; per file median 0.036 s vs
0.226 s, max 0.82 s vs 4.83 s.

Experiment 1 (archived) is the same flags on the first 100 files and
reports median 0.97, min 0.91 as a per-file figure. The median is unchanged at ten times
the sample; only the tail is longer, part of it from taking a minimum over
2.2x more pages.

The four pages below 0.90 fall in `0000527` (0.8509), `0000750` (0.8807)
and `0000908` (0.8978); the next files down are `0000877`, `0000365`,
`0000316`. Inspected by eye, all are the same font-rasterization
difference: identical text, identical line breaks, ghosting on every glyph
edge. The score tracks text density rather than severity - the worst page
is a dense small-type statistical table, where far more pixels sit on a
glyph edge (15.8% differing strongly, against ~10% on a typical text page).

Rendering under `chromium/8021` and `chromium/8035` was byte-identical on
these files (max channel difference 0 on every page), so the earlier
experiments' quality figures still stand at 8035.

## Behavior notes

Expect occasional 1-pixel size differences where `pt * dpi / 72` is an exact
integer (A4 at 300 DPI: 2482 vs 2483); at 72 DPI the scale is exactly 1 and
none appear. `pdftoppm` also reports syntax problems it finds in damaged
files; this tool is silent unless a page fails.

## Limits of the metrics

`sim` is `1 - mean|diff|/255`: it catches page-scale damage (missing images,
wrong rotation, walls of .notdef boxes), not a single wrong word; it
measures agreement between the two tools, not correctness where both make
the same mistake; and structured-but-small differences score high - a full
red/blue channel swap only drops it to ~0.96, which is why the harness also
runs an explicit swapped-channel check. `ssim` (Wang et al. 2004, gaussian
11x11, validated against scikit-image) spreads the tail far wider - pages
clustered at 0.85-0.92 under `sim` separate across 0.5-0.85 - but it is
not simply the better metric. Being a local-contrast measure, it
over-penalizes dark and low-contrast photographic content: the corpus's
worst `ssim` page (0.49) is a near-black product photo the two tools render
indistinguishably, with 0.8% of pixels differing strongly. Median ink rises
monotonically as `ssim` falls (0.11 at `ssim` >= 0.9, 0.28 below 0.6), so on
a scan-heavy corpus `ssim` alone flags mostly images. `ink_delta`, the gap in
non-white pixel fraction, catches content one tool did not draw, which a
page-wide mean dilutes to nothing on a mostly white page; it is the only one
of the three that flags a hairline-pattern difference `sim` scores above its
own median.

Use them together: among the 20 worst pages by each metric, `ssim` and `sim`
share 6, `ssim` and `ink_delta` share 4, `sim` and `ink_delta` share 6. Each
surfaces a substantially different set, and none of the three is a
sufficient detector on its own. Pages a tool timed out on are excluded from
comparison.

## Earlier measurements

Experiments 1 and 2 (August 2026, pdfiumtoppm 0.1.1/0.1.2, pdfium
`chromium/8021`) are in
[pdftoppm-comparison-2026-08.md](pdftoppm-comparison-2026-08.md). They
remain the source for the speed breakdown - rasterization ~1.8x, PDF-to-PNG
~7.4x at matched encoder effort - and for the `-png-compress` trade-off.
