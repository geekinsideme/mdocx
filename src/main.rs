use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use clap::Parser;
use anyhow::{Context, anyhow};
use filetime::{FileTime, set_file_mtime};
use walkdir::WalkDir;

mod converter;

#[derive(Parser, Debug)]
#[command(name = "mdocx")]
#[command(version)]
#[command(about = "Converts between Markdown and DOCX formats", long_about = None)]
struct Args {
    /// Input file paths / directories / wildcard patterns
    inputs: Vec<String>,

    /// Output file path (single output file)
    #[arg(short = 'o', long = "out")]
    output: Option<PathBuf>,

    /// Output directory for per-file conversion results
    #[arg(short = 'd', long = "directory")]
    output_directory: Option<PathBuf>,

    /// Specify source format filters (repeatable, e.g., -f docx -f md or -f c -f h)
    #[arg(short = 'f', long = "from")]
    from_format: Vec<String>,

    /// Explicitly specify the target format ('md' or 'docx')
    #[arg(short = 't', long = "to")]
    to_format: Option<String>,

    /// Append target extension after original extension when output is auto-generated (e.g., a.docx -> a.docx.md)
    #[arg(short = 'a', long = "apend-suffix", visible_alias = "append-suffix")]
    apend_suffix: bool,

    /// Skip conversion when input/output timestamps are already identical
    #[arg(short = 'c', long = "check-timestamp")]
    check_timestamp: bool,

    /// Recursively process subdirectories when directory input is specified
    #[arg(short = 'r', long = "recursive")]
    recursive: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Format {
    Markdown,
    PlainText,
    Docx,
}

#[derive(Debug, Clone)]
struct CollectedInput {
    path: PathBuf,
    relative_path: PathBuf,
}

fn detect_format(path: &Path, flag: Option<&str>) -> Result<Format, anyhow::Error> {
    if let Some(f) = flag {
        match f.trim().trim_start_matches('.').to_lowercase().as_str() {
            "md" | "markdown" => return Ok(Format::Markdown),
            "docx" => return Ok(Format::Docx),
            _ => return Ok(Format::PlainText),
        }
    }

    // Autodetect from extension
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        match ext.to_lowercase().as_str() {
            "md" | "markdown" => Ok(Format::Markdown),
            "docx" => Ok(Format::Docx),
            _ => Ok(Format::PlainText),
        }
    } else {
        Ok(Format::PlainText)
    }
}

fn build_output_path(input_path: &Path, to_fmt: Format, append_suffix: bool) -> PathBuf {
    let new_ext = match to_fmt {
        Format::Markdown => "md",
        Format::PlainText => "txt",
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

fn detect_target_format(flag: &str) -> Result<Format, anyhow::Error> {
    match flag.trim().trim_start_matches('.').to_lowercase().as_str() {
        "md" | "markdown" => Ok(Format::Markdown),
        "docx" => Ok(Format::Docx),
        _ => Err(anyhow!(
            "Unsupported target format specified: {} (use md/markdown or docx)",
            flag
        )),
    }
}

fn reverse_format(fmt: Format) -> Format {
    match fmt {
        Format::Markdown | Format::PlainText => Format::Docx,
        Format::Docx => Format::Markdown,
    }
}

fn preprocess_md_like_input(from_fmt: Format, content: String) -> String {
    if from_fmt == Format::PlainText {
        content.replace('\t', "    ")
    } else {
        content
    }
}

fn has_wildcard(input: &str) -> bool {
    input.contains('*') || input.contains('?') || input.contains('[')
}

fn wildcard_base_dir(pattern: &str) -> PathBuf {
    let wildcard_idx = pattern.find(['*', '?', '[']);
    if let Some(idx) = wildcard_idx {
        let prefix = &pattern[..idx];
        let sep_idx = prefix.rfind(['/', '\\']);
        let base = sep_idx.map(|i| &prefix[..i]).unwrap_or(".");
        if base.is_empty() {
            PathBuf::from(".")
        } else {
            PathBuf::from(base)
        }
    } else {
        Path::new(pattern)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

fn explicit_input_relative_path(raw: &str, path: &Path) -> PathBuf {
    let raw_path = Path::new(raw);
    if !raw_path.is_absolute() {
        return raw_path.to_path_buf();
    }

    if let Ok(cwd) = std::env::current_dir()
        && let Ok(rel) = path.strip_prefix(&cwd)
    {
        return rel.to_path_buf();
    }

    path.file_name().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("input"))
}

fn normalize_filter_token(token: &str) -> Vec<String> {
    let t = token.trim().trim_start_matches('.').to_lowercase();
    match t.as_str() {
        "md" | "markdown" => vec!["md".to_string(), "markdown".to_string()],
        "docx" => vec!["docx".to_string()],
        _ if !t.is_empty() => vec![t],
        _ => Vec::new(),
    }
}

fn build_from_extension_filters(from_filters: &[String]) -> HashSet<String> {
    let mut set = HashSet::new();
    for f in from_filters {
        for ext in normalize_filter_token(f) {
            set.insert(ext);
        }
    }
    set
}

fn path_matches_extensions(path: &Path, extensions: &HashSet<String>) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| extensions.contains(&e.to_lowercase()))
        .unwrap_or(false)
}

fn explicit_single_source_format(from_filters: &[String]) -> Option<&str> {
    if from_filters.len() != 1 {
        return None;
    }
    Some(from_filters[0].as_str())
}

fn push_if_match(
    path: PathBuf,
    relative_path: PathBuf,
    from_ext_filter: Option<&HashSet<String>>,
    seen: &mut HashSet<PathBuf>,
    output: &mut Vec<CollectedInput>,
) {
    if let Some(extensions) = from_ext_filter
        && !path_matches_extensions(&path, extensions)
    {
        return;
    }

    if seen.insert(path.clone()) {
        output.push(CollectedInput { path, relative_path });
    }
}

fn collect_from_directory(
    root_base: &Path,
    dir: &Path,
    from_ext_filter: &HashSet<String>,
    recursive: bool,
    seen: &mut HashSet<PathBuf>,
    output: &mut Vec<CollectedInput>,
) {
    let walker = if recursive {
        WalkDir::new(dir)
    } else {
        WalkDir::new(dir).max_depth(1)
    };

    for entry in walker.into_iter().filter_map(Result::ok) {
        let p = entry.path();
        if p.is_file() {
            let relative = p
                .strip_prefix(root_base)
                .map(Path::to_path_buf)
                .unwrap_or_else(|_| p.file_name().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("input")));
            push_if_match(p.to_path_buf(), relative, Some(from_ext_filter), seen, output);
        }
    }
}

fn collect_source_files(
    inputs: &[String],
    from_filters: &[String],
    recursive: bool,
) -> Result<Vec<CollectedInput>, anyhow::Error> {
    let from_ext_filter = build_from_extension_filters(from_filters);
    let has_filter = !from_ext_filter.is_empty();

    let mut files = Vec::new();
    let mut seen = HashSet::new();

    for raw in inputs {
        if has_wildcard(raw) {
            if !has_filter {
                return Err(anyhow!(
                    "Wildcard input requires at least one -f/--from filter: {}",
                    raw
                ));
            }

            let mut matched = false;
            for entry in glob::glob(raw)
                .with_context(|| format!("Invalid wildcard pattern: {}", raw))?
            {
                let path = entry.with_context(|| format!("Failed to read wildcard match: {}", raw))?;
                matched = true;
                let base_dir = wildcard_base_dir(raw);

                if path.is_dir() {
                    collect_from_directory(&base_dir, &path, &from_ext_filter, recursive, &mut seen, &mut files);
                } else if path.is_file() {
                    let relative = path
                        .strip_prefix(&base_dir)
                        .map(Path::to_path_buf)
                        .unwrap_or_else(|_| path.file_name().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("input")));
                    push_if_match(path, relative, Some(&from_ext_filter), &mut seen, &mut files);
                }
            }

            if !matched {
                return Err(anyhow!("No files matched wildcard pattern: {}", raw));
            }
            continue;
        }

        let path = PathBuf::from(raw);
        if path.is_dir() {
            if !has_filter {
                return Err(anyhow!(
                    "Directory input requires -f/--from to filter files: {}",
                    path.display()
                ));
            }
            collect_from_directory(&path, &path, &from_ext_filter, recursive, &mut seen, &mut files);
        } else if path.is_file() {
            let relative = explicit_input_relative_path(raw, &path);
            push_if_match(path, relative, if has_filter { Some(&from_ext_filter) } else { None }, &mut seen, &mut files);
        } else {
            return Err(anyhow!("Input not found: {}", path.display()));
        }
    }

    if files.is_empty() {
        return Err(anyhow!("No input files to process."));
    }

    Ok(files)
}

fn resolve_output_path(
    input_path: &Path,
    relative_path: &Path,
    to_fmt: Format,
    append_suffix: bool,
    output_dir_opt: Option<&PathBuf>,
) -> Result<PathBuf, anyhow::Error> {
    if let Some(out) = output_dir_opt {
        if out.exists() {
            if !out.is_dir() {
                return Err(anyhow!(
                    "When -d/--directory is provided, it must be a directory: {}",
                    out.display()
                ));
            }
        } else {
            fs::create_dir_all(out)
                .with_context(|| format!("Failed to create output directory: {}", out.display()))?;
        }

        let rel = if relative_path.is_absolute() {
            input_path
                .file_name()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("input"))
        } else {
            relative_path.to_path_buf()
        };
        let generated = build_output_path(&rel, to_fmt, append_suffix);
        return Ok(out.join(generated));
    }

    Ok(build_output_path(input_path, to_fmt, append_suffix))
}

fn org_path_display(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn section_with_org_path(org_path: &Path, body: &str) -> String {
    let mut section = String::new();
    section.push_str("###  ");
    section.push_str(&org_path_display(org_path));
    section.push_str("\n\n");
    section.push_str(body.trim_end());
    section.push_str("\n\n");
    section
}

fn latest_input_mtime(paths: &[PathBuf]) -> Result<FileTime, anyhow::Error> {
    let mut latest: Option<FileTime> = None;
    for p in paths {
        let t = file_mtime(p)?;
        latest = match latest {
            Some(cur) if cur >= t => Some(cur),
            _ => Some(t),
        };
    }
    latest.ok_or_else(|| anyhow!("No input files to determine timestamp"))
}

fn all_inputs_match_output_timestamp(inputs: &[PathBuf], output: &Path) -> Result<bool, anyhow::Error> {
    if !output.exists() {
        return Ok(false);
    }
    for input in inputs {
        if !timestamps_match(input, output)? {
            return Ok(false);
        }
    }
    Ok(true)
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

    if args.output.is_some() && args.output_directory.is_some() {
        return Err(anyhow!("Please specify either -o/--out or -d/--directory, not both."));
    }

    if args.inputs.is_empty() {
        return Err(anyhow!("At least one input path is required."));
    }

    let input_files = collect_source_files(&args.inputs, &args.from_format, args.recursive)
        .context("Failed to collect input files")?;

    let is_batch_input = args.inputs.len() > 1
        || args.inputs.iter().any(|raw| has_wildcard(raw) || Path::new(raw).is_dir())
        || input_files.len() > 1;

    if is_batch_input && args.output.is_some() {
        let combined_output_path = args
            .output
            .as_ref()
            .ok_or_else(|| anyhow!("Missing -o/--out output path"))?
            .clone();

        let mut input_paths = Vec::with_capacity(input_files.len());
        for item in &input_files {
            input_paths.push(item.path.clone());
        }

        let mut sections_md = String::new();
        let mut combined_to_fmt: Option<Format> = None;

        for item in &input_files {
            let input_path = &item.path;
            let from_fmt = detect_format(input_path, explicit_single_source_format(&args.from_format))
                .with_context(|| format!("Failed to determine source format: {}", input_path.display()))?;

            let to_fmt = if let Some(to_flag) = args.to_format.as_deref() {
                detect_target_format(to_flag)
                    .with_context(|| format!("Failed to determine target format for {}", input_path.display()))?
            } else {
                reverse_format(from_fmt)
            };

            if let Some(existing) = combined_to_fmt {
                if existing != to_fmt {
                    return Err(anyhow!(
                        "Batch inputs resolve to different output formats. Please specify -t explicitly."
                    ));
                }
            } else {
                combined_to_fmt = Some(to_fmt);
            }

            let body_md = match (from_fmt, to_fmt) {
                (Format::Markdown, Format::Docx) | (Format::PlainText, Format::Docx) => {
                    let md_like = fs::read_to_string(input_path)
                        .with_context(|| format!("Failed to read markdown file: {}", input_path.display()))?;
                    preprocess_md_like_input(from_fmt, md_like)
                }
                (Format::Docx, Format::Markdown) => {
                    let docx_bytes = fs::read(input_path)
                        .with_context(|| format!("Failed to read DOCX file: {}", input_path.display()))?;
                    let media_dir = combined_output_path
                        .parent()
                        .unwrap_or_else(|| Path::new("."))
                        .join("media");
                    let source_docx_stem = input_path.file_stem().and_then(|s| s.to_str());
                    converter::docx_to_md(&docx_bytes, Some(&media_dir), source_docx_stem)
                        .context("Error converting DOCX to Markdown")?
                }
                (Format::PlainText, Format::Markdown) => {
                    let text = fs::read_to_string(input_path)
                        .with_context(|| format!("Failed to read text file: {}", input_path.display()))?;
                    preprocess_md_like_input(from_fmt, text)
                }
                _ => {
                    return Err(anyhow!(
                        "Unsupported conversion pair in batch mode: {:?} -> {:?} for {}",
                        from_fmt,
                        to_fmt,
                        input_path.display()
                    ));
                }
            };

            sections_md.push_str(&section_with_org_path(&item.relative_path, &body_md));
        }

        let to_fmt = combined_to_fmt.ok_or_else(|| anyhow!("No input files to process."))?;

        if let Some(parent) = combined_output_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create output parent directory: {}", parent.display()))?;
        }

        if args.check_timestamp
            && all_inputs_match_output_timestamp(&input_paths, &combined_output_path)?
        {
            println!(
                "Skipping conversion because timestamps are identical for all inputs: {}",
                combined_output_path.display()
            );
            return Ok(());
        }

        match to_fmt {
            Format::Docx => {
                let docx_bytes = converter::md_to_docx(&sections_md)
                    .context("Error converting combined Markdown to DOCX")?;
                fs::write(&combined_output_path, docx_bytes)
                    .with_context(|| format!("Failed to write DOCX file: {}", combined_output_path.display()))?;
            }
            Format::Markdown => {
                fs::write(&combined_output_path, sections_md)
                    .with_context(|| format!("Failed to write Markdown file: {}", combined_output_path.display()))?;
            }
            Format::PlainText => {
                return Err(anyhow!("PlainText target is not supported for combined output."));
            }
        }

        let latest = latest_input_mtime(&input_paths)?;
        set_file_mtime(&combined_output_path, latest)
            .with_context(|| format!("Failed to set output timestamp: {}", combined_output_path.display()))?;

        println!(
            "Conversion completed successfully! processed={}, converted=1, skipped=0",
            input_files.len()
        );
        return Ok(());
    }

    let total = input_files.len();
    let mut converted = 0usize;
    let mut skipped = 0usize;

    for item in &input_files {
        let input_path = &item.path;
        let from_fmt = detect_format(input_path, explicit_single_source_format(&args.from_format))
            .with_context(|| format!("Failed to determine source format: {}", input_path.display()))?;

        let to_fmt = if let Some(to_flag) = args.to_format.as_deref() {
            detect_target_format(to_flag)
                .with_context(|| format!("Failed to determine target format for {}", input_path.display()))?
        } else {
            reverse_format(from_fmt)
        };

        if from_fmt == to_fmt {
            return Err(anyhow!(
                "Source and target formats are the same for {}. Check -f/--from and -t/--to.",
                input_path.display()
            ));
        }

        let output_path = resolve_output_path(
            input_path,
            &item.relative_path,
            to_fmt,
            args.apend_suffix,
            args.output_directory.as_ref(),
        )?;

        let output_path = if let Some(out_file) = args.output.as_ref() {
            out_file.clone()
        } else {
            output_path
        };

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create output parent directory: {}", parent.display()))?;
        }

        if args.check_timestamp && output_path.exists() && timestamps_match(input_path, &output_path)? {
            println!(
                "Skipping conversion because timestamps are identical: {} == {}",
                input_path.display(),
                output_path.display()
            );
            skipped += 1;
            continue;
        }

        println!(
            "Converting {} ({:?}) -> {} ({:?})...",
            input_path.display(),
            from_fmt,
            output_path.display(),
            to_fmt
        );

        match (from_fmt, to_fmt) {
            (Format::Markdown, Format::Docx) | (Format::PlainText, Format::Docx) => {
                let md_content = fs::read_to_string(input_path)
                    .with_context(|| format!("Failed to read markdown file: {}", input_path.display()))?;
                let md_content = preprocess_md_like_input(from_fmt, md_content);
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

        converted += 1;
    }

    println!(
        "Conversion completed successfully! processed={}, converted={}, skipped={}",
        total, converted, skipped
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{build_from_extension_filters, build_output_path, copy_mtime, detect_format, has_wildcard, path_matches_extensions, preprocess_md_like_input, timestamps_match, Format};
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
    fn wildcard_detection_works() {
        assert!(has_wildcard("*.docx"));
        assert!(has_wildcard("file?.md"));
        assert!(has_wildcard("[ab].md"));
        assert!(!has_wildcard("notes.md"));
    }

    #[test]
    fn extension_filter_supports_multiple_from_values() {
        let filters = build_from_extension_filters(&["c".to_string(), "h".to_string()]);
        assert!(path_matches_extensions(Path::new("a.c"), &filters));
        assert!(path_matches_extensions(Path::new("a.h"), &filters));
        assert!(!path_matches_extensions(Path::new("a.md"), &filters));
    }

    #[test]
    fn markdown_alias_filter_expands_known_text_extensions() {
        let filters = build_from_extension_filters(&["md".to_string()]);
        assert!(path_matches_extensions(Path::new("a.md"), &filters));
        assert!(path_matches_extensions(Path::new("a.markdown"), &filters));
        assert!(!path_matches_extensions(Path::new("a.txt"), &filters));
    }

    #[test]
    fn detect_format_treats_unknown_from_flag_as_plain_text() {
        let fmt = detect_format(Path::new("dummy.unknown"), Some("rs")).expect("should parse format");
        assert_eq!(fmt, Format::PlainText);

        let fmt_c = detect_format(Path::new("dummy.unknown"), Some("c")).expect("should parse format");
        assert_eq!(fmt_c, Format::PlainText);
    }

    #[test]
    fn detect_format_autodetects_non_special_extensions_as_plain_text() {
        let rs_fmt = detect_format(Path::new("main.rs"), None).expect("autodetect rs should work");
        assert_eq!(rs_fmt, Format::PlainText);

        let no_ext_fmt = detect_format(Path::new("README"), None).expect("autodetect no extension should work");
        assert_eq!(no_ext_fmt, Format::PlainText);

        let md_fmt = detect_format(Path::new("note.md"), None).expect("autodetect md should work");
        assert_eq!(md_fmt, Format::Markdown);

        let docx_fmt = detect_format(Path::new("a.docx"), None).expect("autodetect docx should work");
        assert_eq!(docx_fmt, Format::Docx);
    }

    #[test]
    fn preprocess_expands_tabs_only_for_non_md_text_inputs() {
        let src = "a\tb\n\tc".to_string();
        let c_result = preprocess_md_like_input(Format::PlainText, src.clone());
        let md_result = preprocess_md_like_input(Format::Markdown, src);

        assert_eq!(c_result, "a    b\n    c");
        assert_eq!(md_result, "a\tb\n\tc");
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
