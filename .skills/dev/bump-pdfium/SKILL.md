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

---
name: bump-pdfium
description: Move to a new pdfium-binaries release (and, if needed, pdfium-render).
---

# Bump pdfium

1. Pick the release tag at https://github.com/bblanchon/pdfium-binaries/releases
   (`chromium/NNNN`). Check pdfium-render's changelog for a matching minimum.
2. Download `pdfium-linux-x64.tgz`, `pdfium-linux-arm64.tgz`, and
   `pdfium-win-x64.tgz` for that tag; record each `sha256sum`.
3. Update, all together:
   - `.github/workflows/ci.yml` and `release.yml`: `PDFIUM_RELEASE` and all
     three matrix `sha256` values.
   - `README.md` "libpdfium": the pinned version and the curl URL.
   - `CHANGES.md`: new pdfium version (and `VERSION` file's MAJOR.MINOR.BUILD).
   - `Cargo.toml` pins if pdfium-render or image change; run `cargo update -p`.
4. Extract the x64 tarball locally and run `PDFIUM_PATH=<dir>/lib tests/smoke.sh`.
5. Run `tests/compare.py` against a corpus sample if rendering behavior may
   have changed; update `docs/pdftoppm-comparison.md` only if numbers moved.
