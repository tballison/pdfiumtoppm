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
# needs PDFIUM_PATH -> dir containing the platform pdfium library
set -euo pipefail
export LC_ALL=C
B=${1:-target/release/pdfiumtoppm}
T=$(dirname "$0")
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }
file_size() {
  stat -c %s "$1" 2>/dev/null || stat -f %z "$1"
}

"$B" -png -r 300 -scale-to 4096 -gray -f 1 -l 1 "$T/twelve.pdf" "$W/t"
[ -f "$W/t-01.png" ] || fail "expected t-01.png"
[ "$(od -An -tx1 -N 8 "$W/t-01.png" | tr -d ' \n')" = 89504e470d0a1a0a ] || fail "not a PNG"

# pHYs: 300 dpi = 11811 px/m = 0x00002e23, twice, unit 1
"$B" -png -r 300 -f 1 -l 1 "$T/twelve.pdf" "$W/phys"
grep -q pHYs "$W/phys-01.png" || fail "png should carry a pHYs chunk"
od -An -tx1 -v "$W/phys-01.png" | tr -d ' \n' | grep -q '7048597300002e2300002e2301' || fail "pHYs should say 300 dpi"
"$B" -png -scale-to 842 -f 1 -l 1 "$T/tika-testPDF.pdf" "$W/phys72"
od -An -tx1 -v "$W/phys72-1.png" | tr -d ' \n' | grep -q '7048597300000b1300000b1301' || fail "pHYs should follow -scale-to (72 dpi = 2835 px/m)"

"$B" -png -png-compress 9 -f 1 -l 1 "$T/twelve.pdf" "$W/c9"
"$B" -png -png-compress 0 -f 1 -l 1 "$T/twelve.pdf" "$W/c0"
[ "$(file_size "$W/c9-01.png")" -lt "$(file_size "$W/c0-01.png")" ] || fail "png-compress 9 should be smaller than 0"
for bad in "-png-compress 10" "-png-compress -1" "-png-compress x"; do
  "$B" $bad "$T/twelve.pdf" "$W/x" 2>/dev/null && rc=0 || rc=$?
  [ "$rc" = 99 ] || fail "'$bad' should exit 99 (got $rc)"
done

# JFIF density 300 dpi: units 1, 0x012c twice
"$B" -jpeg -r 300 -f 1 -l 1 "$T/twelve.pdf" "$W/j"
[ "$(od -An -tx1 -N 3 "$W/j-01.jpg" | tr -d ' \n')" = ffd8ff ] || fail "not a JPEG"
od -An -tx1 -N 20 "$W/j-01.jpg" | tr -d ' \n' | grep -q '4a46494600010[12]01012c012c' || fail "JFIF should say 300 dpi"
"$B" -jpeg -jpegopt quality=10 -r 72 "$T/tika-testPDF.pdf" "$W/jq10"
"$B" -jpeg -jpegopt quality=95,progressive=y,optimize=y -r 72 "$T/tika-testPDF.pdf" "$W/jq95"
[ "$(file_size "$W/jq10-1.jpg")" -lt "$(file_size "$W/jq95-1.jpg")" ] || fail "quality 10 should be smaller than 95"
od -An -tx1 -v "$W/jq95-1.jpg" | tr -d ' \n' | grep -q ffc2 || fail "progressive=y should write SOF2"
"$B" -jpeg -gray -r 72 "$T/redblue.pdf" "$W/jg"
[ -f "$W/jg-1.jpg" ] || fail "expected jg-1.jpg"
for bad in "-jpeg -png" "-jpeg -jpegopt quality=101" "-jpeg -jpegopt foo=1" "-jpeg -jpegopt progressive=maybe"; do
  "$B" $bad "$T/twelve.pdf" "$W/x" 2>/dev/null && rc=0 || rc=$?
  [ "$rc" = 99 ] || fail "'$bad' should exit 99 (got $rc)"
done
"$B" -jpegopt quality=50 -r 72 -f 1 -l 1 "$T/twelve.pdf" "$W/jo" 2>"$W/jo.err" || fail "-jpegopt without -jpeg should only warn"
grep -q "only valid with jpeg" "$W/jo.err" && [ -f "$W/jo-01.ppm" ] || fail "-jpegopt without -jpeg should warn and write ppm"

"$B" -r 72 "$T/twelve.pdf" "$W/p"
p_pages=("$W"/p-*.ppm)
[ "${#p_pages[@]}" = 12 ] || fail "expected 12 ppm files"
[ "$(head -c 2 "$W/p-01.ppm")" = P6 ] || fail "expected P6"

"$B" -r 72 -gray "$T/twelve.pdf" "$W/g"
[ "$(head -c 2 "$W/g-01.pgm")" = P5 ] || fail "expected P5"

"$B" -r 72 -f 3 -max-pages 4 "$T/twelve.pdf" "$W/m"
[ "$(ls "$W"/m-*.ppm | tr '\n' ' ')" = "$W/m-03.ppm $W/m-04.ppm $W/m-05.ppm $W/m-06.ppm " ] || fail "max-pages"

"$B" -r 300 -max-pixels 4000000 "$T/huge.pdf" "$W/h" 2>/dev/null
[ "$(head -c 20 "$W/h-1.ppm" | sed -n 2p)" = "2000 2000 255" ] || fail "max-pixels"

"$B" -r 72 -max-memory 0 -f 1 -l 1 "$T/twelve.pdf" "$W/mem0" || fail "-max-memory 0 should disable the limit"
if [ "$(uname -s)" = Darwin ]; then
  "$B" -max-memory 64 "$T/twelve.pdf" "$W/mem" 2>"$W/mem.err" && rc=0 || rc=$?
  [ "$rc" = 99 ] && grep -q "not supported on macos" "$W/mem.err" || fail "-max-memory should be rejected on macOS (rc $rc)"
  "$B" -r 72 -f 1 -l 1 "$T/twelve.pdf" "$W/memn" 2>"$W/memn.err" || fail "default run should apply no memory limit on macOS: $(head -1 "$W/memn.err")"
else
  "$B" -r 72 -max-memory 64 -f 1 -l 1 "$T/twelve.pdf" "$W/mem" || fail "64 MiB should suffice for a small page"
  "$B" -r 300 "$T/huge.pdf" "$W/memd" 2>"$W/memd.err" && rc=0 || rc=$?
  [ "$rc" = 4 ] && grep -q "over -max-memory" "$W/memd.err" || fail "default -max-memory should catch the huge page (rc $rc)"
  "$B" -r 300 -max-memory 256 "$T/huge.pdf" "$W/memh" 2>"$W/memh.err" && rc=0 || rc=$?
  [ "$rc" = 4 ] && [ ! -f "$W/memh-1.ppm" ] && grep -q "over -max-memory" "$W/memh.err" || fail "over -max-memory should skip the page and exit 4 (rc $rc)"
  "$B" -r 300 -max-memory 256 -max-pixels 4000000 "$T/huge.pdf" "$W/memp" 2>/dev/null || fail "-max-pixels should keep huge page under -max-memory"
fi

"$B" -r 72 "$T/degenerate.pdf" "$W/d" 2>/dev/null && rc=0 || rc=$?
[ "$rc" = 0 ] || fail "bad page should not fail the run (exit $rc)"
[ -f "$W/d-1.ppm" ] && [ ! -f "$W/d-2.ppm" ] && [ -f "$W/d-3.ppm" ] || fail "expected pages 1 and 3 only"
"$B" -r 72 -f 2 -l 2 "$T/degenerate.pdf" "$W/d2" 2>/dev/null && rc=0 || rc=$?
[ "$rc" = 99 ] || fail "range with no renderable page should exit 99 (got $rc)"
"$B" -r 72 -strict "$T/degenerate.pdf" "$W/ds" 2>/dev/null && rc=0 || rc=$?
[ "$rc" = 99 ] || fail "-strict with a bad page should exit 99 (got $rc)"
[ -f "$W/ds-1.ppm" ] && [ -f "$W/ds-3.ppm" ] || fail "-strict should still render the good pages"
"$B" -r 72 -strict -f 1 -l 1 "$T/degenerate.pdf" "$W/ds1" || fail "-strict with no bad page should exit 0"
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
