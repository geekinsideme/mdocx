use std::path::{Path, PathBuf};
use std::fs;
use clap::Parser;
use anyhow::{Context, anyhow};

mod converter;

#[derive(Parser, Debug)]
#[command(name = "mdocx")]
#[command(version = "0.1.0")]
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
            "md" | "markdown" | "txt" => Ok(Format::Markdown),
            "docx" => Ok(Format::Docx),
            _ => Err(anyhow!("Could not autodetect format from file extension '.{}'. Please specify using -f/--from or -t/--to.", ext)),
        }
    } else {
        Err(anyhow!("File has no extension. Please specify format explicitly using -f/--from or -t/--to."))
    }
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
        None => {
            let mut path = input_path.clone();
            let new_ext = match to_fmt {
                Format::Markdown => "md",
                Format::Docx => "docx",
            };
            path.set_extension(new_ext);
            path
        }
    };

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

            let md_content = converter::docx_to_md(&docx_bytes, Some(&media_dir))
                .context("Error converting DOCX to Markdown")?;
            fs::write(&output_path, md_content)
                .with_context(|| format!("Failed to write Markdown file: {}", output_path.display()))?;
        }
        _ => unreachable!(),
    }

    println!("Conversion completed successfully!");
    Ok(())
}
