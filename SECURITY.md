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

# Security

## Scope

This tool renders untrusted PDFs and is expected to be run under an external
timeout with resource limits (`-max-memory` is a convenience, not a sandbox). 
Crashes, hangs, and excessive memory or CPU use
on malicious input are therefore **not** security issues here. `-max-pages` and
`-max-pixels` bound output size and `-max-memory` bounds address space (exit 4 when hit);
nothing bounds time. Under `-max-memory`, a fatal signal is classified as
out-of-memory (exit 4) or crash (exit 99) by how full the address space was;
that is a heuristic, so a crash reported as exit 4 may still be a PDFium
bug worth a sample.

In scope: anything that lets a PDF or command line escape those expectations,
such as writing outside the requested output path, loading a `libpdfium`
from an unintended location, or code execution in this tool's own code.

## Reporting

Report in-scope issues privately via "Report a vulnerability" on this
repository's Security tab. You should hear back within a week.

In most cases, security reports should be made to
[Chromium security team](https://www.chromium.org/Home/chromium-security/reporting-security-bugs/); let us know so the pinned `pdfium-binaries` release 
can be bumped once a fix ships.
