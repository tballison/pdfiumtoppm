// Copyright 2026 Tim Allison
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! pdftoppm-style page renderer on PDFium. Unsupported flags are rejected, not ignored.

use std::env;
use std::path::{Path, PathBuf};
use std::process::exit;

use std::fs::File;
use std::io::BufWriter;

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::codecs::pnm::{PnmEncoder, PnmSubtype, SampleEncoding};
use image::{DynamicImage, ImageEncoder};
use pdfium_render::prelude::*;

// exit codes mirror pdftoppm
const EXIT_OPEN_PDF: i32 = 1;
const EXIT_OPEN_OUTPUT: i32 = 2;
const EXIT_OTHER: i32 = 99;

const USAGE: &str = "\
Usage: pdfiumtoppm [options] <PDF-file> <image-root>
  -f <int>           : first page to print
  -l <int>           : last page to print
  -r <fp>            : resolution, in DPI (default is 150)
  -scale-to <int>    : scales each page to fit within scale-to*scale-to pixel box
  -max-pages <int>   : render at most this many pages
  -max-pixels <int>  : downscale any page whose width*height would exceed this
  -png               : generate a PNG file (default is PPM)
  -png-compress <int>: PNG zlib level 0-9 (default 1; pdftoppm uses 6)
  -gray              : generate a grayscale image file
  -opw <string>      : owner password (for encrypted files)
  -upw <string>      : user password (for encrypted files)
  -pdfium <path>     : directory containing libpdfium.so
  -v                 : print version info
  -h                 : print usage information
Environment:
  PDFIUM_PATH        : directory containing libpdfium.so
libpdfium.so is searched in: -pdfium, $PDFIUM_PATH, the executable's directory,
then the system library path.";

struct Opts {
    first: Option<u32>,
    last: Option<u32>,
    dpi: f32,
    scale_to: Option<u32>,
    max_pages: Option<u32>,
    max_pixels: Option<u64>,
    png: bool,
    png_compress: u8,
    gray: bool,
    opw: Option<String>,
    upw: Option<String>,
    pdfium_dir: Option<PathBuf>,
    pdf: PathBuf,
    root: String,
}

fn usage_err(msg: &str) -> ! {
    eprintln!("Error: {msg}\n{USAGE}");
    exit(EXIT_OTHER);
}

fn parse_args() -> Opts {
    let mut args = env::args().skip(1);
    let mut first = None;
    let mut last = None;
    let mut dpi: f32 = 150.0;
    let mut scale_to = None;
    let mut max_pages = None;
    let mut max_pixels = None;
    let mut png = false;
    let mut png_compress: u8 = 1;
    let mut gray = false;
    let mut opw = None;
    let mut upw = None;
    let mut pdfium_dir = None;
    let mut positional: Vec<String> = Vec::new();

    fn value(args: &mut impl Iterator<Item = String>, flag: &str) -> String {
        args.next()
            .unwrap_or_else(|| usage_err(&format!("{flag} requires a value")))
    }
    fn num<T: std::str::FromStr>(v: &str, flag: &str) -> T {
        v.parse()
            .unwrap_or_else(|_| usage_err(&format!("bad value for {flag}: {v}")))
    }

    while let Some(a) = args.next() {
        match a.as_str() {
            "-f" => first = Some(num(&value(&mut args, "-f"), "-f")),
            "-l" => last = Some(num(&value(&mut args, "-l"), "-l")),
            "-r" => dpi = num(&value(&mut args, "-r"), "-r"),
            "-scale-to" => scale_to = Some(num(&value(&mut args, "-scale-to"), "-scale-to")),
            "-max-pages" => max_pages = Some(num(&value(&mut args, "-max-pages"), "-max-pages")),
            "-max-pixels" => {
                max_pixels = Some(num(&value(&mut args, "-max-pixels"), "-max-pixels"))
            }
            "-png" => png = true,
            "-png-compress" => {
                png_compress = num(&value(&mut args, "-png-compress"), "-png-compress")
            }
            "-gray" => gray = true,
            "-opw" => opw = Some(value(&mut args, "-opw")),
            "-upw" => upw = Some(value(&mut args, "-upw")),
            "-pdfium" => pdfium_dir = Some(PathBuf::from(value(&mut args, "-pdfium"))),
            "-v" => {
                println!("pdfiumtoppm version {}", env!("CARGO_PKG_VERSION"));
                exit(0);
            }
            "-h" | "-help" | "--help" | "-?" => {
                println!("pdfiumtoppm version {}\n{USAGE}", env!("CARGO_PKG_VERSION"));
                exit(0);
            }
            s if s.starts_with('-') && s.len() > 1 => usage_err(&format!("unsupported option {s}")),
            _ => positional.push(a),
        }
    }
    if positional.len() != 2 {
        usage_err("expected <PDF-file> <image-root>");
    }
    if !(dpi > 0.0 && dpi.is_finite()) {
        usage_err("-r must be a positive number");
    }
    if png_compress > 9 {
        usage_err("-png-compress must be 0-9");
    }
    for (flag, zero) in [
        ("-scale-to", scale_to == Some(0)),
        ("-max-pages", max_pages == Some(0)),
        ("-max-pixels", max_pixels == Some(0)),
    ] {
        if zero {
            usage_err(&format!("{flag} must be positive"));
        }
    }
    let root = positional.pop().unwrap();
    let pdf = PathBuf::from(positional.pop().unwrap());
    Opts {
        first,
        last,
        dpi,
        scale_to,
        max_pages,
        max_pixels,
        png,
        png_compress,
        gray,
        opw,
        upw,
        pdfium_dir,
        pdf,
        root,
    }
}

fn bind_pdfium(explicit: Option<&Path>) -> Pdfium {
    // explicit -pdfium must bind: no silent fallback to another library
    if let Some(d) = explicit {
        let lib = Pdfium::pdfium_platform_library_name_at_path(d);
        return match Pdfium::bind_to_library(&lib) {
            Ok(b) => Pdfium::new(b),
            Err(e) => {
                eprintln!("Error: could not load {}: {e:?}", lib.display());
                exit(EXIT_OTHER);
            }
        };
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = env::var_os("PDFIUM_PATH") {
        dirs.push(PathBuf::from(d));
    }
    if let Some(d) = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        dirs.push(d);
    }

    let mut errors = Vec::new();
    for d in &dirs {
        let lib = Pdfium::pdfium_platform_library_name_at_path(d);
        match Pdfium::bind_to_library(&lib) {
            Ok(b) => return Pdfium::new(b),
            Err(e) => errors.push(format!("  {}: {e:?}", lib.display())),
        }
    }
    match Pdfium::bind_to_system_library() {
        Ok(b) => Pdfium::new(b),
        Err(e) => {
            errors.push(format!("  system library: {e:?}"));
            eprintln!("Error: could not load libpdfium:\n{}", errors.join("\n"));
            exit(EXIT_OTHER);
        }
    }
}

fn main() {
    let opts = parse_args();
    let pdfium = bind_pdfium(opts.pdfium_dir.as_deref());

    // pdfium takes one password; pdftoppm accepts either, so try both
    let passwords: Vec<Option<&str>> = if opts.upw.is_none() && opts.opw.is_none() {
        vec![None]
    } else {
        [opts.upw.as_deref(), opts.opw.as_deref()]
            .into_iter()
            .flatten()
            .map(Some)
            .collect()
    };
    let mut doc = None;
    let mut last_err = None;
    for pw in passwords {
        match pdfium.load_pdf_from_file(&opts.pdf, pw) {
            Ok(d) => {
                doc = Some(d);
                break;
            }
            Err(e) => last_err = Some(e),
        }
    }
    let doc = match doc {
        Some(d) => d,
        None => {
            eprintln!(
                "Error: could not open {}: {:?}",
                opts.pdf.display(),
                last_err.unwrap()
            );
            exit(EXIT_OPEN_PDF);
        }
    };

    let n_pages = doc.pages().len() as u32;
    let first = opts.first.unwrap_or(1).max(1);
    let mut last = opts.last.unwrap_or(n_pages).min(n_pages);
    if let Some(m) = opts.max_pages {
        last = last.min(first.saturating_add(m).saturating_sub(1));
    }
    if first > last {
        eprintln!("Error: wrong page range given: the first page ({first}) can not be after the last page ({last})");
        exit(EXIT_OTHER);
    }
    let pad = n_pages.to_string().len();
    let ext = match (opts.png, opts.gray) {
        (true, _) => "png",
        (false, true) => "pgm",
        (false, false) => "ppm",
    };

    // like pdftoppm: skip bad pages; nonzero exit only if nothing rendered
    let (mut failed, mut ok) = (0u32, 0u32);
    for pg in first..=last {
        let out = format!("{}-{:0pad$}.{ext}", opts.root, pg);
        match render_page(&doc, pg, &opts) {
            Ok(img) => {
                if let Err(e) = write_image(&img, &out, &opts) {
                    eprintln!("Error: could not write {out}: {e}");
                    exit(EXIT_OPEN_OUTPUT);
                }
                ok += 1;
            }
            Err(e) => {
                eprintln!("Error: page {pg}: {e}");
                failed += 1;
            }
        }
    }
    if failed > 0 {
        eprintln!("Error: {failed} page(s) failed to render");
        if ok == 0 {
            exit(EXIT_OTHER);
        }
    }
}

fn render_page(doc: &PdfDocument, pg: u32, opts: &Opts) -> Result<DynamicImage, String> {
    let page = doc
        .pages()
        .get((pg - 1) as PdfPageIndex)
        .map_err(|e| format!("could not load: {e:?}"))?;
    let (w, h) = (page.width().value, page.height().value);
    if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
        return Err(format!("degenerate page size {w}x{h} pt"));
    }
    let mut scale = match opts.scale_to {
        Some(s) => s as f32 / w.max(h),
        None => opts.dpi / 72.0,
    };
    if let Some(max) = opts.max_pixels {
        let area = (w as f64 * scale as f64) * (h as f64 * scale as f64);
        if area > max as f64 {
            scale *= (max as f64 / area).sqrt() as f32;
            eprintln!("Warning: page {pg}: downscaled to fit -max-pixels {max}");
        }
    }
    // pdftoppm rounds up
    let (mut w_px, mut h_px) = ((w * scale).ceil() as i32, (h * scale).ceil() as i32);
    if opts
        .max_pixels
        .is_some_and(|m| w_px as u64 * h_px as u64 > m)
    {
        (w_px, h_px) = ((w * scale) as i32, (h * scale) as i32);
    }
    if w_px < 1 || h_px < 1 {
        return Err(format!("image size {w_px}x{h_px} too small"));
    }
    let config = PdfRenderConfig::new()
        .set_fixed_size(w_px, h_px)
        .render_form_data(true)
        .render_annotations(true)
        .use_grayscale_rendering(opts.gray);
    let img = page
        .render_with_config(&config)
        .and_then(|b| b.as_image())
        .map_err(|e| format!("could not render: {e:?}"))?;
    Ok(if opts.gray {
        DynamicImage::ImageLuma8(img.into_luma8())
    } else {
        DynamicImage::ImageRgb8(img.into_rgb8())
    })
}

fn write_image(
    img: &DynamicImage,
    out: &str,
    opts: &Opts,
) -> Result<(), Box<dyn std::error::Error>> {
    let w = BufWriter::new(File::create(out)?);
    if opts.png {
        let level = CompressionType::Level(opts.png_compress);
        PngEncoder::new_with_quality(w, level, FilterType::Adaptive).write_image(
            img.as_bytes(),
            img.width(),
            img.height(),
            img.color().into(),
        )?;
        return Ok(());
    }
    // binary P6/P5 like pdftoppm; image's default is PAM (P7)
    let subtype = if img.color().has_color() {
        PnmSubtype::Pixmap(SampleEncoding::Binary)
    } else {
        PnmSubtype::Graymap(SampleEncoding::Binary)
    };
    PnmEncoder::new(w).with_subtype(subtype).write_image(
        img.as_bytes(),
        img.width(),
        img.height(),
        img.color().into(),
    )?;
    Ok(())
}
