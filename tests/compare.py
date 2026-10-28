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
try:
    from scipy.ndimage import gaussian_filter
except ImportError:
    gaussian_filter = None


def ssim(x, y):
    """Mean SSIM (Wang et al. 2004) over grayscale float arrays: gaussian 11x11
    window, sigma 1.5, K1=0.01, K2=0.03, L=255. Structural rather than per-pixel,
    so it separates 'same content, rasterized differently' from 'content missing',
    which a mean absolute difference cannot."""
    C1, C2 = (0.01 * 255) ** 2, (0.03 * 255) ** 2
    g = lambda v: gaussian_filter(v, sigma=1.5, truncate=3.5)
    ux, uy = g(x), g(y)
    vx, vy, vxy = g(x * x) - ux * ux, g(y * y) - uy * uy, g(x * y) - ux * uy
    num = (2 * ux * uy + C1) * (2 * vxy + C2)
    den = (ux * ux + uy * uy + C1) * (vx + vy + C2)
    # drop the window radius at each edge, as the reference implementation does:
    # filtered values there are contaminated by the padding, not by image content
    pad = int(3.5 * 1.5 + 0.5)
    m = (num / den)[pad:-pad, pad:-pad] if min(x.shape) > 2 * pad else num / den
    return float(np.mean(m))

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
    """(similarity, similarity with fa's R and B swapped, ink_a, ink_b). Mean pixel
    distance barely moves on a channel swap over a mostly white page, so the swapped
    score is the check: if it is higher, the channels are in the wrong order. Ink is
    the non-white fraction; a large gap means one tool did not draw something, which
    a page-wide mean dilutes to nothing on a mostly white page."""
    a, b = Image.open(fa).convert("RGB"), Image.open(fb).convert("RGB")
    if a.size != b.size:
        return None
    x, y = (np.asarray(i, dtype=np.float32) for i in (a, b))
    s = float(1 - np.mean(np.abs(x - y)) / 255)
    ink = lambda v: float((v.min(axis=2) < 250).mean())
    ss = ssim(x.mean(axis=2), y.mean(axis=2)) if gaussian_filter else ""
    return s, float(1 - np.mean(np.abs(x[..., ::-1] - y)) / 255), ink(x), ink(y), ss

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("pdf_dir")
    ap.add_argument("-n", type=int, default=0, help="limit to first N files (sorted)")
    ap.add_argument("--flags", default="-r 72 -png -f 1 -l 3")
    ap.add_argument("--pdfiumtoppm", default="target/release/pdfiumtoppm")
    ap.add_argument("--pdftoppm", default="pdftoppm")
    ap.add_argument("--timeout", type=int, default=120)
    ap.add_argument("--min-sim", type=float, default=0.9, help="flag pages below this")
    ap.add_argument("--keep-below", type=float, default=None, metavar="SIM",
                    help="copy both renders of any page scoring below SIM into --keep-dir")
    ap.add_argument("--keep-dir", default="lowpages")
    a = ap.parse_args()
    pdfs = sorted(glob.glob(os.path.join(a.pdf_dir, "*.pdf")))
    if a.n:
        pdfs = pdfs[: a.n]
    flags = a.flags.split()
    work = tempfile.mkdtemp(prefix="compare-")
    rows = []
    page_sims = []
    page_rows = []
    if a.keep_below is not None:
        os.makedirs(a.keep_dir, exist_ok=True)
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
                        pg = os.path.basename(fa).rsplit("-", 1)[-1][:-4]
                        page_rows.append(dict(file=n, page=pg, sim=round(ss[0], 4),
                                              ssim=round(ss[4], 4) if ss[4] != "" else "",
                                              swap_sim=round(ss[1], 4),
                                              ink_a=round(ss[2], 4), ink_b=round(ss[3], 4),
                                              ink_delta=round(abs(ss[2] - ss[3]), 4),
                                              w=(da or ("", ""))[0], h=(da or ("", ""))[1]))
                        if a.keep_below is not None and ss[0] < a.keep_below:
                            for src, tag in ((fa, "pdfiumtoppm"), (fb, "pdftoppm")):
                                shutil.copy(src, os.path.join(
                                    a.keep_dir, f"{n}-p{pg}-{ss[0]:.4f}-{tag}.png"))
                        sims.append(ss[0])
                        swapped += ss[1] > ss[0] + 0.001
            low = sims
            page_sims.extend(sims)
            rows.append(dict(file=n, rc_a=ra[0], rc_b=rb[0], pages_a=len(ra[2]), pages_b=len(rb[2]),
                             t_a=round(ra[1], 3), t_b=round(rb[1], 3),
                             pages_cmp=len(sims),
                             mean_sim=round(statistics.mean(sims), 4) if sims else "",
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
    if page_rows:
        with open("compare-pages.csv", "w", newline="") as f:
            w = csv.DictWriter(f, fieldnames=page_rows[0].keys())
            w.writeheader()
            w.writerows(page_rows)
    ta, tb = [r["t_a"] for r in rows], [r["t_b"] for r in rows]
    print(f"\n{len(rows)} files. rc mismatches: {sum(r['rc_a'] != r['rc_b'] for r in rows)}, "
          f"page-count mismatches: {sum(r['pages_a'] != r['pages_b'] for r in rows)}, "
          f"dimension mismatches: {sum(bool(r['dim_mismatch']) for r in rows)}, "
          f"channel-swapped files: {sum(bool(r['channel_swap']) for r in rows)}")
    print(f"time pdfiumtoppm total {sum(ta):.1f}s median {statistics.median(ta):.3f}s max {max(ta):.2f}s | "
          f"pdftoppm total {sum(tb):.1f}s median {statistics.median(tb):.3f}s max {max(tb):.2f}s")
    if Image:
        if page_sims:
            print(f"similarity, per page ({len(page_sims)} pages): median "
                  f"{statistics.median(page_sims):.4f} mean {statistics.mean(page_sims):.4f} "
                  f"min {min(page_sims):.4f}; "
                  f"{sum(x < a.min_sim for x in page_sims)} page(s) below {a.min_sim}")
        s = [r["min_sim"] for r in rows if r["min_sim"] != ""]
        if s:
            print(f"similarity, per file (worst page of each): median {statistics.median(s):.4f} "
                  f"min {min(s):.4f}; "
                  f"{sum(x < a.min_sim for x in s)} file(s) below {a.min_sim}")
    else:
        print("Pillow/numpy not installed: dimensions and similarity skipped")
    print("details: compare.csv" + (", compare-pages.csv" if page_rows else ""))

if __name__ == "__main__":
    main()
