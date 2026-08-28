#!/usr/bin/env python3
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
"""Run pdfiumtoppm and pdftoppm (-cropbox) over a directory of PDFs; compare
exit codes, page counts, dimensions, wall time, and (with Pillow+numpy)
per-page similarity. Writes compare.csv. See README.
"""
import argparse, csv, glob, os, shutil, statistics, subprocess, sys, tempfile, time

try:
    from PIL import Image
    import numpy as np
except ImportError:
    Image = None

def run(cmd, root, timeout):
    t = time.time()
    try:
        p = subprocess.run(cmd + [root], capture_output=True, text=True, timeout=timeout)
        rc, err = p.returncode, p.stderr.strip().splitlines()[:1]
    except subprocess.TimeoutExpired:
        rc, err = -1, ["TIMEOUT"]
    return rc, time.time() - t, sorted(glob.glob(root + "-*")), " ".join(err)[:200]

def size(f):
    if Image:
        return Image.open(f).size
    return None

def sim(fa, fb):
    """(similarity, similarity with fa's R and B swapped). Mean pixel distance barely
    moves on a channel swap over a mostly white page, so the swapped score is the
    check: if it is higher, the channels are in the wrong order."""
    a, b = Image.open(fa).convert("RGB"), Image.open(fb).convert("RGB")
    if a.size != b.size:
        return None
    x, y = (np.asarray(i, dtype=np.float32) for i in (a, b))
    s = float(1 - np.mean(np.abs(x - y)) / 255)
    return s, float(1 - np.mean(np.abs(x[..., ::-1] - y)) / 255)

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("pdf_dir")
    ap.add_argument("-n", type=int, default=0, help="limit to first N files (sorted)")
    ap.add_argument("--flags", default="-r 72 -png -f 1 -l 3")
    ap.add_argument("--pdfiumtoppm", default="target/release/pdfiumtoppm")
    ap.add_argument("--pdftoppm", default="pdftoppm")
    ap.add_argument("--timeout", type=int, default=120)
    ap.add_argument("--min-sim", type=float, default=0.9, help="flag pages below this")
    a = ap.parse_args()
    pdfs = sorted(glob.glob(os.path.join(a.pdf_dir, "*.pdf")))
    if a.n:
        pdfs = pdfs[: a.n]
    flags = a.flags.split()
    work = tempfile.mkdtemp(prefix="compare-")
    rows = []
    try:
        for pdf in pdfs:
            n = os.path.splitext(os.path.basename(pdf))[0]
            ra = run([a.pdfiumtoppm] + flags + [pdf], f"{work}/{n}-a", a.timeout)
            rb = run([a.pdftoppm, "-cropbox"] + flags + [pdf], f"{work}/{n}-b", a.timeout)
            sims, dims, swapped = [], [], 0
            for fa, fb in zip(ra[2], rb[2]):
                da, db = size(fa), size(fb)
                if da != db:
                    dims.append(f"{da}!={db}")
                if Image and fa.endswith(".png"):
                    ss = sim(fa, fb)
                    if ss:
                        sims.append(ss[0])
                        swapped += ss[1] > ss[0] + 0.001
            low = sims
            rows.append(dict(file=n, rc_a=ra[0], rc_b=rb[0], pages_a=len(ra[2]), pages_b=len(rb[2]),
                             t_a=round(ra[1], 3), t_b=round(rb[1], 3),
                             min_sim=round(min(low), 4) if low else "",
                             channel_swap=swapped, dim_mismatch=";".join(dims), err_a=ra[3], err_b=rb[3]))
            r = rows[-1]
            flag = ""
            if r["rc_a"] != r["rc_b"] or r["pages_a"] != r["pages_b"] or dims or (low and min(low) < a.min_sim):
                flag = "  <-- DIFF"
            if swapped:
                flag += f"  <-- R/B SWAPPED on {swapped} page(s)"
            print(f'{n} rc={r["rc_a"]}/{r["rc_b"]} pages={r["pages_a"]}/{r["pages_b"]} '
                  f't={r["t_a"]:.2f}/{r["t_b"]:.2f}s sim={r["min_sim"]}{flag}', flush=True)
            for f in ra[2] + rb[2]:
                os.remove(f)
    finally:
        shutil.rmtree(work, ignore_errors=True)
    if not rows:
        sys.exit("no PDFs found")
    with open("compare.csv", "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=rows[0].keys())
        w.writeheader()
        w.writerows(rows)
    ta, tb = [r["t_a"] for r in rows], [r["t_b"] for r in rows]
    print(f"\n{len(rows)} files. rc mismatches: {sum(r['rc_a'] != r['rc_b'] for r in rows)}, "
          f"page-count mismatches: {sum(r['pages_a'] != r['pages_b'] for r in rows)}, "
          f"dimension mismatches: {sum(bool(r['dim_mismatch']) for r in rows)}, "
          f"channel-swapped files: {sum(bool(r['channel_swap']) for r in rows)}")
    print(f"time pdfiumtoppm total {sum(ta):.1f}s median {statistics.median(ta):.3f}s max {max(ta):.2f}s | "
          f"pdftoppm total {sum(tb):.1f}s median {statistics.median(tb):.3f}s max {max(tb):.2f}s")
    if Image:
        s = [r["min_sim"] for r in rows if r["min_sim"] != ""]
        if s:
            print(f"similarity median {statistics.median(s):.4f} min {min(s):.4f}; "
                  f"{sum(x < a.min_sim for x in s)} file(s) below {a.min_sim}")
    else:
        print("Pillow/numpy not installed: dimensions and similarity skipped")
    print("details: compare.csv")

if __name__ == "__main__":
    main()
