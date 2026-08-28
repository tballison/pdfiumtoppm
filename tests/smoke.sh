#!/usr/bin/env bash
# Copyright 2026 Tim Allison
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.
# needs PDFIUM_PATH -> dir containing libpdfium.so
set -euo pipefail
B=${1:-target/release/pdfiumtoppm}
T=$(dirname "$0")
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

"$B" -png -r 300 -scale-to 4096 -gray -f 1 -l 1 "$T/twelve.pdf" "$W/t"
[ -f "$W/t-01.png" ] || fail "expected t-01.png"
head -c 8 "$W/t-01.png" | grep -q PNG || fail "not a PNG"

"$B" -png -png-compress 9 -f 1 -l 1 "$T/twelve.pdf" "$W/c9"
"$B" -png -png-compress 0 -f 1 -l 1 "$T/twelve.pdf" "$W/c0"
[ "$(stat -c %s "$W/c9-01.png")" -lt "$(stat -c %s "$W/c0-01.png")" ] || fail "png-compress 9 should be smaller than 0"
for bad in "-png-compress 10" "-png-compress -1" "-png-compress x"; do
  "$B" $bad "$T/twelve.pdf" "$W/x" 2>/dev/null && rc=0 || rc=$?
  [ "$rc" = 99 ] || fail "'$bad' should exit 99 (got $rc)"
done

"$B" -r 72 "$T/twelve.pdf" "$W/p"
[ "$(ls "$W"/p-*.ppm | wc -l)" = 12 ] || fail "expected 12 ppm files"
[ "$(head -c 2 "$W/p-01.ppm")" = P6 ] || fail "expected P6"

"$B" -r 72 -gray "$T/twelve.pdf" "$W/g"
[ "$(head -c 2 "$W/g-01.pgm")" = P5 ] || fail "expected P5"

"$B" -r 72 -f 3 -max-pages 4 "$T/twelve.pdf" "$W/m"
[ "$(ls "$W"/m-*.ppm | tr '\n' ' ')" = "$W/m-03.ppm $W/m-04.ppm $W/m-05.ppm $W/m-06.ppm " ] || fail "max-pages"

"$B" -r 300 -max-pixels 4000000 "$T/huge.pdf" "$W/h" 2>/dev/null
[ "$(head -c 20 "$W/h-1.ppm" | sed -n 2p)" = "2000 2000 255" ] || fail "max-pixels"

"$B" -r 72 -max-memory 64 -f 1 -l 1 "$T/twelve.pdf" "$W/mem" || fail "64 MiB should suffice for a small page"
"$B" -r 72 -max-memory 0 -f 1 -l 1 "$T/twelve.pdf" "$W/mem0" || fail "-max-memory 0 should disable the limit"
"$B" -r 300 "$T/huge.pdf" "$W/memd" 2>"$W/memd.err" && rc=0 || rc=$?
[ "$rc" = 4 ] && grep -q "over -max-memory" "$W/memd.err" || fail "default -max-memory should catch the huge page (rc $rc)"
"$B" -r 300 -max-memory 256 "$T/huge.pdf" "$W/memh" 2>"$W/memh.err" && rc=0 || rc=$?
[ "$rc" = 4 ] && [ ! -f "$W/memh-1.ppm" ] && grep -q "over -max-memory" "$W/memh.err" || fail "over -max-memory should skip the page and exit 4 (rc $rc)"
"$B" -r 300 -max-memory 256 -max-pixels 4000000 "$T/huge.pdf" "$W/memp" 2>/dev/null || fail "-max-pixels should keep huge page under -max-memory"

"$B" -r 72 "$T/degenerate.pdf" "$W/d" 2>/dev/null && rc=0 || rc=$?
[ "$rc" = 0 ] || fail "bad page should not fail the run (exit $rc)"
[ -f "$W/d-1.ppm" ] && [ ! -f "$W/d-2.ppm" ] && [ -f "$W/d-3.ppm" ] || fail "expected pages 1 and 3 only"
"$B" -r 72 -f 2 -l 2 "$T/degenerate.pdf" "$W/d2" 2>/dev/null && rc=0 || rc=$?
[ "$rc" = 99 ] || fail "range with no renderable page should exit 99 (got $rc)"
"$B" -r nan "$T/twelve.pdf" "$W/x" 2>/dev/null && rc=0 || rc=$?
[ "$rc" = 99 ] || fail "-r nan should be rejected"
for bad in "-f 20" "-f 5 -l 3" "-max-pages 0" "-scale-to 0" "-max-pixels 0" "-max-memory -1"; do
  "$B" $bad "$T/twelve.pdf" "$W/x" 2>/dev/null && rc=0 || rc=$?
  [ "$rc" = 99 ] || fail "'$bad' should exit 99 (got $rc)"
  ls "$W"/x-* >/dev/null 2>&1 && fail "'$bad' should write nothing"
done
"$B" -pdfium /nonexistent -f 1 -l 1 "$T/twelve.pdf" "$W/x" 2>/dev/null && rc=0 || rc=$?
[ "$rc" = 99 ] || fail "explicit bad -pdfium should not fall back (got $rc)"

# real content: rendered page must not be blank (bytes other than 0xff beyond the header)
"$B" -r 72 "$T/tika-testPDF.pdf" "$W/txt"
[ "$(tr -d '\377' < "$W/txt-1.ppm" | wc -c)" -gt 1000 ] || fail "text page rendered blank"

# channel order: top half red, bottom half blue (72 dpi -> 100x100, header "P6\n100 100\n255\n" = 15 bytes)
"$B" -r 72 "$T/redblue.pdf" "$W/rb"
[ "$(od -An -tx1 -j 15 -N 3 "$W/rb-1.ppm" | tr -d ' ')" = ff0000 ] || fail "top pixel should be red"
[ "$(od -An -tx1 -j $((15 + 99*300)) -N 3 "$W/rb-1.ppm" | tr -d ' ')" = 0000ff ] || fail "bottom pixel should be blue"
"$B" -r 72 -gray "$T/redblue.pdf" "$W/rbg"
[ "$(od -An -tx1 -j 15 -N 1 "$W/rbg-1.pgm" | tr -d ' ')" = 4c ] || fail "red should be luma 76"

P="$T/tika-testPassword4Spaces.pdf"   # password is four spaces
"$B" -r 72 "$P" "$W/enc" 2>/dev/null && fail "encrypted without password should fail"
[ $? = 1 ] || fail "encrypted without password exit code"
"$B" -r 72 -upw wrong "$P" "$W/enc" 2>/dev/null && fail "wrong password should fail"
"$B" -r 72 -upw "    " "$P" "$W/enc-u" || fail "-upw"
"$B" -r 72 -opw "    " "$P" "$W/enc-o" || fail "-opw"
"$B" -r 72 -upw wrong -opw "    " "$P" "$W/enc-uo" || fail "either password should open"
cmp -s "$W/enc-u-1.ppm" "$W/enc-o-1.ppm" || fail "same render regardless of which password"

"$B" -r 72 "$T/nope.pdf" "$W/x" 2>/dev/null && fail "missing file should fail"
[ $? = 1 ] || fail "missing file exit code"
"$B" -aa no "$T/twelve.pdf" "$W/x" 2>/dev/null && fail "unknown flag should fail"
[ $? = 99 ] || fail "unknown flag exit code"

echo OK
