use std::path::{Path, PathBuf};
use std::fs;
use clap::Parser;
use anyhow::{Context, anyhow};
use filetime::{FileTime, set_file_mtime};

mod converter;

#[derive(Parser, Debug)]
#[command(name = "mdocx")]
#[command(version)]
#[command(about = "Converts between Markdown and DOCX formats", long_about = None)]
struct Args {
    /// Input file path (e.g., input.md or input.docx)
    input: PathBuf,

    /// Output file path (optional, auto-generated if omitted)
    output: Option<PathBuf>,

    /// Explicitly specify the source format ('md' or 'docx')
    #[arg(short = 'f', long = "from")]
    from_format: Option<String>,

    /// Explicitly specify the target format ('md' or 'docx')
    #[arg(short = 't', long = "to")]
    to_format: Option<String>,

    /// Append target extension after original extension when output is auto-generated (e.g., a.docx -> a.docx.md)
    #[arg(short = 'a', long = "apend-suffix", visible_alias = "append-suffix")]
    apend_suffix: bool,

    /// Skip conversion when input/output timestamps are already identical
    #[arg(short = 'c', long = "check-timestamp")]
    check_timestamp: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Format {
    Markdown,
    Docx,
}

fn detect_format(path: &Path, flag: Option<&str>) -> Result<Format, anyhow::Error> {
    if let Some(f) = flag {
        match f.to_lowercase().as_str() {
            "md" | "markdown" => return Ok(Format::Markdown),
            "docx" => return Ok(Format::Docx),
            _ => return Err(anyhow!("Unsupported format specified: {}", f)),
        }
    }

    // Autodetect from extension
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        match ext.to_lowercase().as_str() {
            "md" | "markdown" | "txt" | "c" | "h" | "log" => Ok(Format::Markdown),
            "docx" => Ok(Format::Docx),
            _ => Err(anyhow!("Could not autodetect format from file extension '.{}'. Please specify using -f/--from or -t/--to.", ext)),
        }
    } else {
        Err(anyhow!("File has no extension. Please specify format explicitly using -f/--from or -t/--to."))
    }
}

fn build_output_path(input_path: &Path, to_fmt: Format, append_suffix: bool) -> PathBuf {
    let new_ext = match to_fmt {
        Format::Markdown => "md",
        Format::Docx => "docx",
    };

    if append_suffix
        && input_path.extension().is_some()
        && let Some(file_name) = input_path.file_name().and_then(|n| n.to_str())
    {
        let mut output = input_path.to_path_buf();
        output.set_file_name(format!("{}.{}", file_name, new_ext));
        return output;
    }

    let mut output = input_path.to_path_buf();
    output.set_extension(new_ext);
    output
}

fn file_mtime(path: &Path) -> Result<FileTime, anyhow::Error> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("Failed to read metadata: {}", path.display()))?;
    Ok(FileTime::from_last_modification_time(&metadata))
}

fn timestamps_match(source_path: &Path, target_path: &Path) -> Result<bool, anyhow::Error> {
    let src_time = file_mtime(source_path)?;
    let dst_time = file_mtime(target_path)?;
    Ok(src_time == dst_time)
}

fn copy_mtime(source_path: &Path, target_path: &Path) -> Result<(), anyhow::Error> {
    let src_time = file_mtime(source_path)?;
    set_file_mtime(target_path, src_time)
        .with_context(|| format!("Failed to copy timestamp to {}", target_path.display()))?;
    Ok(())
}

fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();

    let input_path = &args.input;
    if !input_path.exists() {
        return Err(anyhow!("Input file does not exist: {}", input_path.display()));
    }

    let from_fmt = detect_format(input_path, args.from_format.as_deref())
        .context("Failed to determine source format")?;

    let to_fmt = match from_fmt {
        Format::Markdown => Format::Docx,
        Format::Docx => Format::Markdown,
    };

    // Determine output path
    let output_path = match args.output {
        Some(path) => path,
        None => build_output_path(input_path, to_fmt, args.apend_suffix),
    };

    if args.check_timestamp && output_path.exists() && timestamps_match(input_path, &output_path)? {
        println!(
            "Skipping conversion because timestamps are identical: {} == {}",
            input_path.display(),
            output_path.display()
        );
        return Ok(());
    }

    println!(
        "Converting {} ({:?}) -> {} ({:?})...",
        input_path.display(),
        from_fmt,
        output_path.display(),
        to_fmt
    );

    match (from_fmt, to_fmt) {
        (Format::Markdown, Format::Docx) => {
            let md_content = fs::read_to_string(input_path)
                .with_context(|| format!("Failed to read markdown file: {}", input_path.display()))?;
            let docx_bytes = converter::md_to_docx(&md_content)
                .context("Error converting Markdown to DOCX")?;
            fs::write(&output_path, docx_bytes)
                .with_context(|| format!("Failed to write DOCX file: {}", output_path.display()))?;
        }
        (Format::Docx, Format::Markdown) => {
            let docx_bytes = fs::read(input_path)
                .with_context(|| format!("Failed to read DOCX file: {}", input_path.display()))?;
            
            let output_parent = output_path.parent().unwrap_or_else(|| Path::new("."));
            let media_dir = output_parent.join("media");
            let source_docx_stem = input_path.file_stem().and_then(|s| s.to_str());

            let md_content = converter::docx_to_md(&docx_bytes, Some(&media_dir), source_docx_stem)
                .context("Error converting DOCX to Markdown")?;
            fs::write(&output_path, md_content)
                .with_context(|| format!("Failed to write Markdown file: {}", output_path.display()))?;
        }
        _ => unreachable!(),
    }

    copy_mtime(input_path, &output_path)
        .context("Failed to copy source file timestamp to output file")?;

    println!("Conversion completed successfully!");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{build_output_path, copy_mtime, timestamps_match, Format};
    use filetime::{FileTime, set_file_mtime};
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, process};

    fn temp_path(prefix: &str) -> PathBuf {
        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("mdocx_{}_{}_{}", prefix, process::id(), uniq))
    }

    #[test]
    fn build_output_path_replaces_extension_by_default() {
        let path = Path::new("a.docx");
        let output = build_output_path(path, Format::Markdown, false);
        assert_eq!(output, Path::new("a.md"));
    }

    #[test]
    fn build_output_path_appends_extension_with_suffix_option() {
        let path = Path::new("a.docx");
        let output = build_output_path(path, Format::Markdown, true);
        assert_eq!(output, Path::new("a.docx.md"));
    }

    #[test]
    fn build_output_path_keeps_default_behavior_without_original_extension() {
        let path = Path::new("a");
        let output = build_output_path(path, Format::Docx, true);
        assert_eq!(output, Path::new("a.docx"));
    }

    #[test]
    fn timestamps_match_detects_equal_and_different_times() {
        let src = temp_path("src_eq");
        let dst = temp_path("dst_eq");

        fs::write(&src, b"src").expect("create source file");
        fs::write(&dst, b"dst").expect("create destination file");

        let t1 = FileTime::from_unix_time(1_700_000_000, 123_000_000);
        let t2 = FileTime::from_unix_time(1_700_000_001, 0);

        set_file_mtime(&src, t1).expect("set src mtime");
        set_file_mtime(&dst, t1).expect("set dst mtime");
        assert!(timestamps_match(&src, &dst).expect("compare equal timestamps"));

        set_file_mtime(&dst, t2).expect("set dst mtime to different value");
        assert!(!timestamps_match(&src, &dst).expect("compare different timestamps"));

        let _ = fs::remove_file(&src);
        let _ = fs::remove_file(&dst);
    }

    #[test]
    fn copy_mtime_copies_source_timestamp_to_target() {
        let src = temp_path("src_copy");
        let dst = temp_path("dst_copy");

        fs::write(&src, b"src").expect("create source file");
        fs::write(&dst, b"dst").expect("create destination file");

        let src_time = FileTime::from_unix_time(1_710_000_000, 999_000_000);
        set_file_mtime(&src, src_time).expect("set src mtime");

        copy_mtime(&src, &dst).expect("copy mtime from src to dst");
        assert!(timestamps_match(&src, &dst).expect("timestamps should match after copy"));

        let _ = fs::remove_file(&src);
        let _ = fs::remove_file(&dst);
    }
}
