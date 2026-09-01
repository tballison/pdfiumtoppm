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
name: release
description: Cut a tagged release; what is automated and what is not.
---

# Release

Tagging is the trigger; everything after it is `release.yml`. Hand the commit
and tag back to the human — never commit, tag, or push.

1. `Cargo.toml`: bump `version`. Run a build so `Cargo.lock` follows.
2. `CHANGES.md`: add a `## <version>` section. The release job extracts this
   section verbatim for the GitHub release notes and **fails if it is
   missing**, so an empty-notes release is not possible.
3. Run the full check from `AGENTS.md`, including `tests/smoke.sh`.
4. Hand back the commit; the human tags `v<version>` and pushes the tag.
   Both `ci` and `release` must be green on all three platforms
   (linux-x64, linux-arm64, win-x64).
5. Verify the published release cold, as a downloader meets it: checksums
   verify, the archive holds the binary next to its pdfium library, and the
   extracted binary renders a page with `PDFIUM_PATH` unset.

Automated by `release.yml`: notes from `CHANGES.md`, the tag/`Cargo.toml`
version guard, all three platform builds, smoke on each, packaging
(`.tar.gz` on Linux, `.zip` on Windows), checksums, and upload.
Dependabot covers cargo and github-actions monthly; pdfium pins are not
automated (see the `bump-pdfium` skill).
