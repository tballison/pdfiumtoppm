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
#[cfg(target_os = "linux")]
use std::sync::atomic::AtomicU64;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use std::fs::File;
use std::io::{BufWriter, Write};

use image::DynamicImage;
use pdfium_render::prelude::*;

// exit codes mirror pdftoppm
const EXIT_OPEN_PDF: i32 = 1;
const EXIT_OPEN_OUTPUT: i32 = 2;
const EXIT_MEMORY: i32 = 4; // ours: -max-memory was hit
const EXIT_TIME: i32 = 5; // ours: -max-time was hit
const EXIT_OTHER: i32 = 99;

const USAGE: &str = "\
Usage: pdfiumtoppm [options] <PDF-file> [<image-root>]
  -f <int>           : first page to print
  -l <int>           : last page to print
  -r <fp>            : resolution, in DPI (default is 150)
  -scale-to <int>    : scales each page to fit within scale-to*scale-to pixel box
  -max-pages <int>   : render at most this many pages
  -max-pixels <int>  : downscale any page whose width*height would exceed this
  -max-memory <int>  : memory limit in MiB (RLIMIT_AS on Linux, a Job Object on
                       Windows; unsupported elsewhere); exit 4 if hit (default:
                       4096 or half of RAM, whichever is lower; 0 disables; on
                       Linux a crash far below the limit exits 99 instead)
  -max-time <fp>     : wall-clock limit in seconds for the whole run; exit 5 if hit
  -png               : generate a PNG file (default is PPM)
  -png-compress <int>: PNG zlib level 0-9 (default 1; pdftoppm uses 6)
  -jpeg              : generate a JPEG file
  -jpegopt <string>  : jpeg options, with format <opt1>=<val1>[,<optN>=<valN>]*
                       (quality=0-100, default 75; progressive=y|n; optimize=y|n)
  -gray              : generate a grayscale image file
  -cropbox           : use the crop box rather than media box (always on here)
  -singlefile        : write only the first page and do not add digits
  -manifest <path>   : write one JSON line per page (status, file, size, dpi, bytes, ms)
  -strict            : exit 99 if any page failed to render
  -opw <string>      : owner password (for encrypted files)
  -upw <string>      : user password (for encrypted files)
  -pdfium <path>     : directory containing the pdfium library
  -v                 : print version info
  -h                 : print usage information
Environment:
  PDFIUM_PATH        : directory containing the pdfium library
The pdfium library (libpdfium.so / libpdfium.dylib / pdfium.dll) is searched
in: -pdfium, $PDFIUM_PATH, the executable's directory, then the system
library path.
Without <image-root>, images are written to stdout.";

struct Opts {
    first: Option<u32>,
    last: Option<u32>,
    dpi: f32,
    scale_to: Option<u32>,
    max_pages: Option<u32>,
    max_pixels: Option<u64>,
    max_memory: Option<u64>,
    max_time: Option<f64>,
    png: bool,
    png_compress: u8,
    jpeg: Option<JpegOpts>,
    gray: bool,
    strict: bool,
    opw: Option<String>,
    upw: Option<String>,
    pdfium_dir: Option<PathBuf>,
    pdf: PathBuf,
    manifest: Option<PathBuf>,
    root: Option<String>, // None = stdout
    singlefile: bool,
}

#[derive(Debug, PartialEq)]
struct JpegOpts {
    quality: u8,
    progressive: bool,
    optimize: bool,
}

// pdftoppm's -jpegopt syntax and defaults
fn parse_jpegopt(s: &str) -> Result<JpegOpts, String> {
    let mut o = JpegOpts {
        quality: 75,
        progressive: false,
        optimize: false,
    };
    for kv in s.split(',').filter(|kv| !kv.is_empty()) {
        let (k, v) = kv
            .split_once('=')
            .ok_or_else(|| format!("bad jpeg option \"{kv}\""))?;
        let yn = |v: &str| match v {
            "y" => Ok(true),
            "n" => Ok(false),
            _ => Err(format!("jpeg {k} option must be \"y\" or \"n\"")),
        };
        match k {
            "quality" => {
                o.quality = v
                    .parse()
                    .ok()
                    .filter(|q| *q <= 100)
                    .ok_or("jpeg quality must be between 0 and 100")?
            }
            "progressive" => o.progressive = yn(v)?,
            "optimize" => o.optimize = yn(v)?,
            _ => return Err(format!("unknown jpeg option \"{k}\"")),
        }
    }
    Ok(o)
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
    let mut max_memory = None;
    let mut max_time = None;
    let mut png = false;
    let mut png_compress: u8 = 1;
    let mut jpeg = false;
    let mut jpegopt = None;
    let mut gray = false;
    let mut strict = false;
    let mut singlefile = false;
    let mut manifest = None;
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
            "-max-time" => max_time = Some(num(&value(&mut args, "-max-time"), "-max-time")),
            "-max-memory" => {
                max_memory = Some(num(&value(&mut args, "-max-memory"), "-max-memory"))
            }
            "-png" => png = true,
            "-png-compress" => {
                png_compress = num(&value(&mut args, "-png-compress"), "-png-compress")
            }
            "-jpeg" => jpeg = true,
            "-jpegopt" => jpegopt = Some(value(&mut args, "-jpegopt")),
            "-gray" => gray = true,
            "-strict" => strict = true,
            "-singlefile" => singlefile = true,
            "-manifest" => manifest = Some(PathBuf::from(value(&mut args, "-manifest"))),
            // pdfium always renders the crop box
            "-cropbox" => {}
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
    if !(1..=2).contains(&positional.len()) {
        usage_err("expected <PDF-file> [<image-root>]");
    }
    if !(dpi > 0.0 && dpi.is_finite()) {
        usage_err("-r must be a positive number");
    }
    if max_time.is_some_and(|t: f64| !(t > 0.0 && t.is_finite())) {
        usage_err("-max-time must be a positive number");
    }
    if png_compress > 9 {
        usage_err("-png-compress must be 0-9");
    }
    if png && jpeg {
        usage_err("-png and -jpeg are exclusive");
    }
    if jpegopt.is_some() && !jpeg {
        eprintln!("Warning: -jpegopt only valid with jpeg output.");
    }
    let jpeg = jpeg
        .then(|| parse_jpegopt(jpegopt.as_deref().unwrap_or("")).unwrap_or_else(|e| usage_err(&e)));
    for (flag, zero) in [
        ("-scale-to", scale_to == Some(0)),
        ("-max-pages", max_pages == Some(0)),
        ("-max-pixels", max_pixels == Some(0)),
    ] {
        if zero {
            usage_err(&format!("{flag} must be positive"));
        }
    }
    let root = (positional.len() == 2).then(|| positional.pop().unwrap());
    let pdf = PathBuf::from(positional.pop().unwrap());
    Opts {
        first,
        last,
        dpi,
        scale_to,
        max_pages,
        max_pixels,
        max_memory,
        max_time,
        png,
        png_compress,
        jpeg,
        gray,
        strict,
        opw,
        upw,
        pdfium_dir,
        pdf,
        manifest,
        root,
        singlefile,
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
    // canonicalize: macOS current_exe() returns a symlink's path, not its target
    if let Some(d) = env::current_exe()
        .and_then(|p| p.canonicalize())
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
            eprintln!(
                "Error: could not load the pdfium library:\n{}",
                errors.join("\n")
            );
            exit(EXIT_OTHER);
        }
    }
}

// elsewhere -max-memory is a usage error and there is no default limit
const MEMORY_LIMIT_SUPPORTED: bool = cfg!(any(windows, target_os = "linux"));

#[cfg(not(any(windows, target_os = "linux")))]
fn limit_memory(_mib: u64) {
    usage_err(&format!(
        "-max-memory is not supported on {}",
        env::consts::OS
    ));
}

#[cfg(not(any(windows, target_os = "linux")))]
fn physical_memory() -> Option<u64> {
    None
}

#[cfg(windows)]
fn physical_memory() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut st: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    st.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    // SAFETY: st is a properly sized struct with dwLength set
    (unsafe { GlobalMemoryStatusEx(&mut st) } != 0).then_some(st.ullTotalPhys)
}

// Job Object commit limit; no crash-code guess as on Unix: if PDFium itself
// dies mid-page the process exits with the OS status, not 4
#[cfg(windows)]
fn limit_memory(mib: u64) {
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_JOB_MEMORY,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    // SAFETY: plain Win32 calls on valid arguments; the job handle lives as long as the process
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_JOB_MEMORY;
        info.JobMemoryLimit = mib.saturating_mul(1 << 20) as usize;
        if job.is_null()
            || SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) == 0
            || AssignProcessToJobObject(job, GetCurrentProcess()) == 0
        {
            eprintln!(
                "Error: -max-memory {mib}: {}",
                std::io::Error::last_os_error()
            );
            exit(EXIT_OTHER);
        }
    }
}

#[cfg(target_os = "linux")]
fn physical_memory() -> Option<u64> {
    // SAFETY: plain sysconf queries
    let (pages, page) = unsafe {
        (
            libc::sysconf(libc::_SC_PHYS_PAGES),
            libc::sysconf(libc::_SC_PAGESIZE),
        )
    };
    (pages > 0 && page > 0).then(|| pages as u64 * page as u64)
}

const DEFAULT_MAX_MEMORY_MIB: u64 = 4096;

// None = no limit: -max-memory 0, or a platform that cannot set one
fn resolve_max_memory(flag: Option<u64>, phys: Option<u64>) -> Option<u64> {
    match flag {
        Some(0) => None,
        Some(m) => Some(m),
        None if MEMORY_LIMIT_SUPPORTED => Some(phys.map_or(DEFAULT_MAX_MEMORY_MIB, |b| {
            DEFAULT_MAX_MEMORY_MIB.min(b >> 21).max(1)
        })),
        None => None,
    }
}

#[cfg(target_os = "linux")]
static MEM_LIMIT: AtomicU64 = AtomicU64::new(0);
#[cfg(target_os = "linux")]
static PAGE_SIZE: AtomicU64 = AtomicU64::new(0);

// VmSize from /proc/self/statm; only async-signal-safe calls, no allocation
#[cfg(target_os = "linux")]
unsafe fn vm_size() -> Option<u64> {
    let fd = libc::open(c"/proc/self/statm".as_ptr(), libc::O_RDONLY);
    if fd < 0 {
        return None;
    }
    let mut buf = [0u8; 32];
    let n = libc::read(fd, buf.as_mut_ptr().cast(), buf.len());
    libc::close(fd);
    let mut pages = 0u64;
    let digits = buf
        .iter()
        .take(n.max(0) as usize)
        .take_while(|b| b.is_ascii_digit());
    let mut any = false;
    for b in digits {
        pages = pages * 10 + u64::from(b - b'0');
        any = true;
    }
    any.then(|| pages * PAGE_SIZE.load(Ordering::Relaxed))
}

#[cfg(target_os = "linux")]
extern "C" fn on_fatal_signal(sig: libc::c_int) {
    let name: &[u8] = match sig {
        libc::SIGABRT => b"SIGABRT",
        libc::SIGTRAP => b"SIGTRAP",
        libc::SIGILL => b"SIGILL",
        libc::SIGSEGV => b"SIGSEGV",
        libc::SIGBUS => b"SIGBUS",
        _ => b"a fatal signal",
    };
    // an allocation failure inside pdfium ends in a crash; tell it apart from an ordinary
    // crash by how close the address space was to the limit (heuristic; unknown counts as OOM)
    let limit = MEM_LIMIT.load(Ordering::Relaxed);
    let oom = unsafe { vm_size() }.is_none_or(|v| v.saturating_mul(3) >= limit.saturating_mul(2));
    let (tail, code): (&[u8], i32) = if oom {
        (
            b" with address space near -max-memory; probably out of memory\n",
            EXIT_MEMORY,
        )
    } else {
        (
            b" well under -max-memory; probably a PDFium bug, not memory\n",
            EXIT_OTHER,
        )
    };
    // SAFETY: write and _exit are async-signal-safe
    unsafe {
        for m in [b"Error: killed by ".as_slice(), name, tail] {
            libc::write(2, m.as_ptr().cast(), m.len());
        }
        libc::_exit(code);
    }
}

#[cfg(target_os = "linux")]
fn limit_memory(mib: u64) {
    let bytes = mib.saturating_mul(1 << 20);
    MEM_LIMIT.store(bytes, Ordering::Relaxed);
    let bytes = bytes as libc::rlim_t;
    let lim = libc::rlimit {
        rlim_cur: bytes,
        rlim_max: bytes,
    };
    // SAFETY: plain syscalls on valid arguments
    unsafe {
        PAGE_SIZE.store(
            libc::sysconf(libc::_SC_PAGESIZE).max(4096) as u64,
            Ordering::Relaxed,
        );
        if libc::setrlimit(libc::RLIMIT_AS, &lim) != 0 {
            eprintln!(
                "Error: -max-memory {mib}: {}",
                std::io::Error::last_os_error()
            );
            exit(EXIT_OTHER);
        }
        for sig in [
            libc::SIGABRT,
            libc::SIGTRAP,
            libc::SIGILL,
            libc::SIGSEGV,
            libc::SIGBUS,
        ] {
            libc::signal(
                sig,
                on_fatal_signal as extern "C" fn(libc::c_int) as libc::sighandler_t,
            );
        }
    }
}

static CURRENT_PAGE: AtomicU32 = AtomicU32::new(0);
static CURRENT_PART: Mutex<Option<String>> = Mutex::new(None);
static MANIFEST: Mutex<Option<File>> = Mutex::new(None);

#[derive(Default)]
struct PageRecord<'a> {
    page: u32,
    status: &'a str,
    file: Option<&'a str>,
    // width, height, effective dpi, downscaled by -max-pixels
    size: Option<(u32, u32, f32, bool)>,
    bytes: Option<u64>,
    ms: Option<u128>,
    error: Option<&'a str>,
}

fn json_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn manifest_line(r: &PageRecord) -> String {
    let mut o = format!("{{\"page\":{},\"status\":{}", r.page, json_str(r.status));
    if let Some(f) = r.file {
        o += &format!(",\"file\":{}", json_str(f));
    }
    if let Some((w, h, dpi, down)) = r.size {
        o += &format!(",\"width\":{w},\"height\":{h},\"dpi\":{dpi:.2},\"downscaled\":{down}");
    }
    if let Some(b) = r.bytes {
        o += &format!(",\"bytes\":{b}");
    }
    if let Some(ms) = r.ms {
        o += &format!(",\"ms\":{ms}");
    }
    if let Some(e) = r.error {
        o += &format!(",\"error\":{}", json_str(e));
    }
    o + "}\n"
}

// one write per line, so a killed run leaves only whole records
fn record(r: &PageRecord) {
    let mut m = MANIFEST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(f) = m.as_mut() {
        if let Err(e) = f.write_all(manifest_line(r).as_bytes()) {
            eprintln!("Error: could not write manifest: {e}");
            hard_exit(EXIT_OPEN_OUTPUT);
        }
    }
}

// pdfium cannot abort a render, so the only way out of a stuck page is ending the process
fn start_watchdog(secs: f64) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs_f64(secs));
        let part = CURRENT_PART.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(p) = part.as_deref() {
            let _ = std::fs::remove_file(p);
        }
        match CURRENT_PAGE.load(Ordering::SeqCst) {
            0 => eprintln!("Error: -max-time {secs}s exceeded before rendering"),
            pg => {
                eprintln!("Error: -max-time {secs}s exceeded during page {pg}");
                record(&PageRecord {
                    page: pg,
                    status: "timeout",
                    ..Default::default()
                });
            }
        }
        hard_exit(EXIT_TIME);
    });
}

// skip atexit and static destructors: pdfium is still running on the main thread
fn hard_exit(code: i32) -> ! {
    #[cfg(unix)]
    // SAFETY: _exit only terminates the process
    unsafe {
        libc::_exit(code)
    }
    #[cfg(windows)]
    // SAFETY: terminating our own process
    unsafe {
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, TerminateProcess};
        TerminateProcess(GetCurrentProcess(), code as u32);
        unreachable!()
    }
    #[cfg(not(any(unix, windows)))]
    exit(code)
}

fn main() {
    let mut opts = parse_args();
    if let Some(t) = opts.max_time {
        start_watchdog(t);
    }
    opts.max_memory = resolve_max_memory(opts.max_memory, physical_memory());
    if let Some(m) = opts.max_memory {
        limit_memory(m); // before libpdfium is loaded, so its allocations count
    }
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
    let (first, last) = match page_range(n_pages, opts.first, opts.last, opts.max_pages) {
        Ok(r) => r,
        Err((first, last)) => {
            eprintln!("Error: wrong page range given: the first page ({first}) can not be after the last page ({last})");
            exit(EXIT_OTHER);
        }
    };
    let last = if opts.singlefile && last > first {
        eprintln!(
            "Warning: Single file will write only the first of the {} pages.",
            last - first + 1
        );
        first
    } else {
        last
    };
    let ext = match (opts.png, opts.jpeg.is_some(), opts.gray) {
        (true, _, _) => "png",
        (_, true, _) => "jpg",
        (_, _, true) => "pgm",
        _ => "ppm",
    };

    if let Some(p) = &opts.manifest {
        match File::create(p) {
            Ok(f) => *MANIFEST.lock().unwrap() = Some(f),
            Err(e) => {
                eprintln!("Error: could not write {}: {e}", p.display());
                exit(EXIT_OPEN_OUTPUT);
            }
        }
    }

    // like pdftoppm: skip bad pages; nonzero exit only if nothing rendered, or -strict
    let (mut failed, mut ok, mut oom) = (0u32, 0u32, 0u32);
    for pg in first..=last {
        CURRENT_PAGE.store(pg, Ordering::SeqCst);
        let out = opts
            .root
            .as_deref()
            .map(|r| out_name(r, pg, n_pages, ext, opts.singlefile));
        let start = Instant::now();
        let mut rec = PageRecord {
            page: pg,
            file: out.as_deref(),
            ..Default::default()
        };
        let mut msg = String::new();
        match render_page(&doc, pg, &opts) {
            Ok((img, dpi, downscaled)) => {
                rec.size = Some((img.width(), img.height(), dpi, downscaled));
                match write_image(&img, dpi, out.as_deref(), &opts) {
                    Ok(n) => {
                        rec.status = "ok";
                        rec.bytes = Some(n);
                        ok += 1;
                    }
                    Err(e) => {
                        msg = format!(
                            "could not write {}: {e}",
                            out.as_deref().unwrap_or("stdout")
                        );
                        eprintln!("Error: {msg}");
                        rec.status = "failed";
                        rec.error = Some(&msg);
                        rec.ms = Some(start.elapsed().as_millis());
                        record(&rec);
                        exit(EXIT_OPEN_OUTPUT);
                    }
                }
            }
            Err(PageError::Memory(need)) => {
                msg = format!("needs about {} MiB, over -max-memory", need >> 20);
                eprintln!("Error: page {pg}: {msg}");
                rec.status = "memory";
                oom += 1;
            }
            Err(PageError::Other(e)) => {
                msg = e;
                eprintln!("Error: page {pg}: {msg}");
                rec.status = "failed";
                failed += 1;
            }
        }
        if rec.status != "ok" {
            rec.file = None;
            rec.error = Some(&msg);
        }
        rec.ms = Some(start.elapsed().as_millis());
        record(&rec);
    }
    if oom > 0 {
        eprintln!("Error: {oom} page(s) skipped for -max-memory");
        exit(EXIT_MEMORY);
    }
    if failed > 0 {
        eprintln!("Error: {failed} page(s) failed to render");
        if ok == 0 || opts.strict {
            exit(EXIT_OTHER);
        }
    }
}

enum PageError {
    Memory(usize),
    Other(String),
}

impl From<String> for PageError {
    fn from(s: String) -> Self {
        PageError::Other(s)
    }
}

// binary P6/P5 with pdftoppm's exact header layout; maxval on its own line
fn pnm_header(color: bool, w: u32, h: u32) -> String {
    format!("P{}\n{w} {h}\n255\n", if color { 6 } else { 5 })
}

// in place, dropping row padding: each output byte lands at or before its source
fn bgr_to_rgb_or_gray(buf: &mut Vec<u8>, w: usize, h: usize, stride: usize, gray: bool) {
    let mut o = 0;
    for y in 0..h {
        for x in 0..w {
            let i = y * stride + x * 3;
            let (b, g, r) = (buf[i] as u32, buf[i + 1] as u32, buf[i + 2] as u32);
            if gray {
                buf[o] = ((r * 299 + g * 587 + b * 114) / 1000) as u8;
                o += 1;
            } else {
                buf[o..o + 3].copy_from_slice(&[r as u8, g as u8, b as u8]);
                o += 3;
            }
        }
    }
    buf.truncate(o);
}

// reserve without touching, so RLIMIT_AS is checked before pdfium allocates
fn probe(bytes: usize) -> Result<Vec<u8>, PageError> {
    let mut v = Vec::new();
    v.try_reserve_exact(bytes)
        .map_err(|_| PageError::Memory(bytes))?;
    Ok(v)
}

// like pdftoppm: N is the PDF page number, zero-padded to the page count's width
fn out_name(root: &str, pg: u32, n_pages: u32, ext: &str, singlefile: bool) -> String {
    if singlefile {
        return format!("{root}.{ext}");
    }
    let pad = n_pages.to_string().len();
    format!("{root}-{pg:0pad$}.{ext}")
}

// clamp like pdftoppm: -f below 1 is 1, -l past the end is the end
fn page_range(
    n_pages: u32,
    first: Option<u32>,
    last: Option<u32>,
    max_pages: Option<u32>,
) -> Result<(u32, u32), (u32, u32)> {
    let first = first.unwrap_or(1).max(1);
    let mut last = last.unwrap_or(n_pages).min(n_pages);
    if let Some(m) = max_pages {
        last = last.min(first.saturating_add(m).saturating_sub(1));
    }
    if first > last {
        Err((first, last))
    } else {
        Ok((first, last))
    }
}

// pixel size for a w x h pt page, the pixels-per-point scale used, and whether -max-pixels shrank it
fn target_size(w: f32, h: f32, opts: &Opts) -> Result<(i32, i32, f32, bool), String> {
    if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
        return Err(format!("degenerate page size {w}x{h} pt"));
    }
    let mut scale = match opts.scale_to {
        Some(s) => s as f32 / w.max(h),
        None => opts.dpi / 72.0,
    };
    let mut downscaled = false;
    if let Some(max) = opts.max_pixels {
        let area = (w as f64 * scale as f64) * (h as f64 * scale as f64);
        if area > max as f64 {
            scale *= (max as f64 / area).sqrt() as f32;
            downscaled = true;
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
    Ok((w_px, h_px, scale, downscaled))
}

// the image and its effective DPI
fn render_page(
    doc: &PdfDocument,
    pg: u32,
    opts: &Opts,
) -> Result<(DynamicImage, f32, bool), PageError> {
    let page = doc
        .pages()
        .get((pg - 1) as PdfPageIndex)
        .map_err(|e| format!("could not load: {e:?}"))?;
    let (w, h) = (page.width().value, page.height().value);
    let (w_px, h_px, scale, downscaled) = match target_size(w, h, opts) {
        Ok((wp, hp, scale, downscaled)) => {
            if downscaled {
                eprintln!(
                    "Warning: page {pg}: downscaled to fit -max-pixels {}",
                    opts.max_pixels.unwrap()
                );
            }
            (wp, hp, scale, downscaled)
        }
        Err(e) => return Err(e.into()),
    };
    let config = PdfRenderConfig::new()
        .set_fixed_size(w_px, h_px)
        // pdfium-render defaults this on, which makes the buffer RGBA while format() still says BGRA
        .set_reverse_byte_order(false)
        .render_form_data(true)
        .render_annotations(true)
        .use_grayscale_rendering(opts.gray);
    // pdfium renders BGR into our buffer, compacted in place: one ~n*3 allocation.
    // Not pdfium's Gray format: it anti-aliases text differently.
    let (w, h) = (w_px as usize, h_px as usize);
    let stride = (w * 3).div_ceil(4) * 4;
    let mut buf = probe(stride * h)?;
    buf.resize(buf.capacity(), 0);
    {
        let mut bitmap = PdfBitmap::from_bytes(w_px, h_px, PdfBitmapFormat::BGR, &mut buf)
            .map_err(|e| format!("could not create bitmap: {e:?}"))?;
        page.render_into_bitmap_with_config(&mut bitmap, &config)
            .map_err(|e| format!("could not render: {e:?}"))?;
    }
    bgr_to_rgb_or_gray(&mut buf, w, h, stride, opts.gray);
    let out = buf;
    let (wu, hu) = (w_px as u32, h_px as u32);
    let img = if opts.gray {
        DynamicImage::ImageLuma8(image::GrayImage::from_raw(wu, hu, out).unwrap())
    } else {
        DynamicImage::ImageRgb8(image::RgbImage::from_raw(wu, hu, out).unwrap())
    };
    Ok((img, scale * 72.0, downscaled))
}

struct Counter<W>(W, u64);

impl<W: Write> Write for Counter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.0.write(buf)?;
        self.1 += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

// via <out>.part and rename, so a crash or full disk never leaves a truncated <out>; returns bytes written
fn write_image(
    img: &DynamicImage,
    dpi: f32,
    out: Option<&str>,
    opts: &Opts,
) -> Result<u64, Box<dyn std::error::Error>> {
    let Some(out) = out else {
        let mut w = BufWriter::new(Counter(std::io::stdout().lock(), 0));
        encode(img, dpi, &mut w, opts)?;
        return Ok(w.into_inner().map_err(|e| e.into_error())?.1);
    };
    let part = format!("{out}.part");
    *CURRENT_PART.lock().unwrap() = Some(part.clone());
    let res = (|| -> Result<u64, Box<dyn std::error::Error>> {
        let mut w = BufWriter::new(Counter(File::create(&part)?, 0));
        encode(img, dpi, &mut w, opts)?;
        let n = w.into_inner().map_err(|e| e.into_error())?.1;
        std::fs::rename(&part, out)?;
        Ok(n)
    })();
    if res.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    *CURRENT_PART.lock().unwrap() = None;
    res
}

fn encode(
    img: &DynamicImage,
    dpi: f32,
    mut w: impl Write,
    opts: &Opts,
) -> Result<(), Box<dyn std::error::Error>> {
    if opts.png {
        let mut enc = png::Encoder::new(w, img.width(), img.height());
        enc.set_color(if img.color().has_color() {
            png::ColorType::Rgb
        } else {
            png::ColorType::Grayscale
        });
        enc.set_depth(png::BitDepth::Eight);
        // same mapping as the image crate's PngEncoder for -png-compress
        if opts.png_compress == 0 {
            enc.set_compression(png::Compression::NoCompression);
        } else {
            enc.set_deflate_compression(png::DeflateCompression::Level(opts.png_compress));
        }
        enc.set_filter(png::Filter::Adaptive);
        // pHYs like pdftoppm: tesseract sizes text from it and misreads without it
        let ppm = (dpi / 0.0254).round() as u32;
        enc.set_pixel_dims(Some(png::PixelDimensions {
            xppu: ppm,
            yppu: ppm,
            unit: png::Unit::Meter,
        }));
        let mut wr = enc.write_header()?;
        wr.write_image_data(img.as_bytes())?;
        // drop would write IEND but swallow its error
        wr.finish()?;
        return Ok(());
    }
    if let Some(j) = &opts.jpeg {
        let (Ok(wu), Ok(hu)) = (u16::try_from(img.width()), u16::try_from(img.height())) else {
            return Err(format!(
                "{}x{} exceeds JPEG's 65535 limit",
                img.width(),
                img.height()
            )
            .into());
        };
        // libjpeg clamps quality 0 to 1
        let mut enc = jpeg_encoder::Encoder::new(w, j.quality.max(1));
        // 4:2:0 at every quality, as libjpeg (pdftoppm) does
        enc.set_sampling_factor(jpeg_encoder::SamplingFactor::F_2_2);
        enc.set_progressive(j.progressive);
        enc.set_optimized_huffman_tables(j.optimize);
        enc.set_density(jpeg_encoder::PixelDensity::dpi(
            dpi.round().clamp(1.0, 65535.0) as u16,
        ));
        let color = if img.color().has_color() {
            jpeg_encoder::ColorType::Rgb
        } else {
            jpeg_encoder::ColorType::Luma
        };
        enc.encode(img.as_bytes(), wu, hu, color)?;
        return Ok(());
    }
    w.write_all(pnm_header(img.color().has_color(), img.width(), img.height()).as_bytes())?;
    w.write_all(img.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(dpi: f32, scale_to: Option<u32>, max_pixels: Option<u64>) -> Opts {
        Opts {
            first: None,
            last: None,
            dpi,
            scale_to,
            max_pages: None,
            max_pixels,
            max_memory: None,
            max_time: None,
            png: false,
            png_compress: 1,
            jpeg: None,
            gray: false,
            strict: false,
            opw: None,
            upw: None,
            pdfium_dir: None,
            pdf: PathBuf::new(),
            manifest: None,
            root: None,
            singlefile: false,
        }
    }

    fn size(r: Result<(i32, i32, f32, bool), String>) -> Result<(i32, i32, bool), String> {
        r.map(|(w, h, _, d)| (w, h, d))
    }

    #[test]
    fn page_range_clamps_like_pdftoppm() {
        assert_eq!(page_range(12, None, None, None), Ok((1, 12)));
        assert_eq!(page_range(12, Some(0), None, None), Ok((1, 12)));
        assert_eq!(page_range(12, Some(3), Some(100), None), Ok((3, 12)));
        assert_eq!(page_range(12, Some(3), None, Some(4)), Ok((3, 6)));
        assert_eq!(page_range(12, Some(3), Some(4), Some(10)), Ok((3, 4)));
        assert_eq!(page_range(12, Some(3), None, Some(u32::MAX)), Ok((3, 12)));
        assert_eq!(
            page_range(12, Some(u32::MAX), None, Some(1)),
            Err((u32::MAX, 12))
        );
        assert_eq!(page_range(12, Some(20), None, None), Err((20, 12)));
        assert_eq!(page_range(12, Some(5), Some(3), None), Err((5, 3)));
        assert_eq!(page_range(0, None, None, None), Err((1, 0)));
    }

    #[test]
    fn out_name_uses_page_number_and_page_count_width() {
        assert_eq!(out_name("p", 3, 12, "ppm", false), "p-03.ppm");
        assert_eq!(out_name("p", 101, 250, "jpg", false), "p-101.jpg");
        assert_eq!(out_name("p", 1, 9, "png", false), "p-1.png");
        assert_eq!(out_name("p", 7, 1000, "png", false), "p-0007.png");
        assert_eq!(out_name("p", 7, 1000, "png", true), "p.png");
    }

    #[test]
    fn jpegopt_parses_like_pdftoppm() {
        let d = JpegOpts {
            quality: 75,
            progressive: false,
            optimize: false,
        };
        assert_eq!(parse_jpegopt(""), Ok(d));
        assert_eq!(
            parse_jpegopt("quality=90,progressive=y,optimize=y"),
            Ok(JpegOpts {
                quality: 90,
                progressive: true,
                optimize: true
            })
        );
        assert_eq!(parse_jpegopt("quality=0").unwrap().quality, 0);
        for bad in [
            "quality=101",
            "quality=x",
            "quality=-1",
            "foo=1",
            "progressive=maybe",
            "quality",
        ] {
            assert!(parse_jpegopt(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn bgr_converts_in_place_and_drops_padding() {
        // 2x2 BGR, stride 8: red, blue | gray, white; 2 padding bytes per row
        let px = [
            0, 0, 255, 255, 0, 0, 9, 9, 128, 128, 128, 255, 255, 255, 9, 9,
        ];
        let mut v = px.to_vec();
        bgr_to_rgb_or_gray(&mut v, 2, 2, 8, false);
        assert_eq!(v, [255, 0, 0, 0, 0, 255, 128, 128, 128, 255, 255, 255]);
        let mut v = px.to_vec();
        bgr_to_rgb_or_gray(&mut v, 2, 2, 8, true);
        assert_eq!(v, [76, 29, 128, 255]);
    }

    #[test]
    fn manifest_lines_are_json() {
        assert_eq!(json_str("a\"b\\c\n\u{1}é"), r#""a\"b\\c\n\u0001é""#);
        let ok = PageRecord {
            page: 3,
            status: "ok",
            file: Some("out-03.png"),
            size: Some((1275, 1650, 150.0, false)),
            bytes: Some(42),
            ms: Some(7),
            error: None,
        };
        assert_eq!(
            manifest_line(&ok),
            "{\"page\":3,\"status\":\"ok\",\"file\":\"out-03.png\",\"width\":1275,\"height\":1650,\"dpi\":150.00,\"downscaled\":false,\"bytes\":42,\"ms\":7}\n"
        );
        let bad = PageRecord {
            page: 2,
            status: "failed",
            error: Some("degenerate page size 0x100 pt"),
            ..Default::default()
        };
        assert_eq!(
            manifest_line(&bad),
            "{\"page\":2,\"status\":\"failed\",\"error\":\"degenerate page size 0x100 pt\"}\n"
        );
    }

    #[test]
    fn pnm_header_matches_pdftoppm() {
        assert_eq!(pnm_header(true, 100, 50), "P6\n100 50\n255\n");
        assert_eq!(pnm_header(false, 7, 9), "P5\n7 9\n255\n");
    }

    #[test]
    fn dpi_rounds_up() {
        // 612x792 pt letter: 1275x1650 exact at 150; A4 at 300 rounds up
        assert_eq!(
            size(target_size(612.0, 792.0, &opts(150.0, None, None))),
            Ok((1275, 1650, false))
        );
        assert_eq!(
            size(target_size(595.276, 841.89, &opts(300.0, None, None))),
            Ok((2481, 3508, false))
        );
        assert_eq!(
            size(target_size(612.0, 792.0, &opts(72.0, None, None))),
            Ok((612, 792, false))
        );
    }

    #[test]
    fn scale_to_fits_long_edge_and_ignores_dpi() {
        assert_eq!(
            size(target_size(612.0, 792.0, &opts(999.0, Some(4096), None))),
            Ok((3166, 4096, false))
        );
        assert_eq!(
            size(target_size(792.0, 612.0, &opts(72.0, Some(4096), None))),
            Ok((4096, 3166, false))
        );
        // enlarges, like pdftoppm
        assert_eq!(
            size(target_size(10.0, 10.0, &opts(72.0, Some(100), None))),
            Ok((100, 100, false))
        );
    }

    #[test]
    fn max_pixels_only_downscales_and_never_exceeds() {
        let (w, h, scale, down) =
            target_size(1000.0, 1000.0, &opts(300.0, None, Some(4_000_000))).unwrap();
        assert_eq!((w, h, down), (2000, 2000, true));
        assert!(
            (scale * 72.0 - 144.0).abs() < 0.01,
            "effective dpi {}",
            scale * 72.0
        );
        assert_eq!(
            size(target_size(
                100.0,
                100.0,
                &opts(72.0, None, Some(4_000_000))
            )),
            Ok((100, 100, false))
        );
        // rounding up would cross the cap; floors instead
        for (w, h, max) in [
            (333.0, 777.0, 100_000u64),
            (612.0, 792.0, 123_457),
            (3.0, 5.0, 8),
        ] {
            let (wp, hp, _, _) = target_size(w, h, &opts(150.0, None, Some(max))).unwrap();
            assert!(
                wp as u64 * hp as u64 <= max,
                "{w}x{h} max {max} -> {wp}x{hp}"
            );
        }
    }

    #[test]
    fn default_max_memory_is_4g_or_half_ram() {
        const G: u64 = 1 << 30;
        assert_eq!(resolve_max_memory(Some(0), Some(32 * G)), None);
        assert_eq!(resolve_max_memory(Some(512), Some(32 * G)), Some(512));
        if MEMORY_LIMIT_SUPPORTED {
            assert_eq!(resolve_max_memory(None, Some(32 * G)), Some(4096));
            assert_eq!(resolve_max_memory(None, Some(4 * G)), Some(2048));
            assert_eq!(resolve_max_memory(None, Some(1)), Some(1));
            assert_eq!(resolve_max_memory(None, None), Some(4096));
        } else {
            assert_eq!(resolve_max_memory(None, Some(32 * G)), None);
        }
    }

    #[test]
    fn degenerate_pages_rejected() {
        for (w, h) in [
            (0.0, 100.0),
            (100.0, -1.0),
            (f32::NAN, 100.0),
            (f32::INFINITY, 100.0),
        ] {
            assert!(target_size(w, h, &opts(72.0, None, None)).is_err());
        }
        // ceil keeps any positive page at least 1x1
        assert_eq!(
            size(target_size(0.001, 0.001, &opts(1.0, None, None))),
            Ok((1, 1, false))
        );
    }
}
