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

# Agent guidance

Skills live in `.skills/dev/<name>/SKILL.md`; read the one matching the task.

## Ground rules

- Never `git commit`, push, tag, or write to GitHub; stage and hand back.
- Before finishing: `cargo fmt --check && cargo clippy --release -- -D warnings
  && cargo build --release && PDFIUM_PATH=<dir>/lib tests/smoke.sh`.
  `smoke.sh` needs a `libpdfium.so`; fetch as in README "libpdfium".
- Test PDFs from external corpora (Digital Corpora etc.) never enter the repo.
  Fixtures under `tests/` are hand-made; keep them tiny.
- `pdftoppm` is the behavioural reference. A deviation is either matched,
  rejected with a usage error, or documented in README "Not supported".
- New flags: README options block, `USAGE` in `src/main.rs`, a smoke case,
  a CHANGES.md line.
- Comments: one terse line, only for a non-obvious WHY.
- Be generous to upstream projects (PDFium, pdfium-render, pdfium-binaries,
  Poppler) in docs; never disparage.
