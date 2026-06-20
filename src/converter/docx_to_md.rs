use docx_rs::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

pub fn docx_to_md(
    docx_bytes: &[u8],
    output_dir: Option<&std::path::Path>,
    source_docx_stem: Option<&str>,
) -> Result<String, anyhow::Error> {
    let doc = read_docx(docx_bytes)?;
    let hyperlinks_json = serde_json::to_value(&doc.hyperlinks).unwrap_or(serde_json::Value::Null);
    let image_counter = Cell::new(1usize);
    let image_name_map = RefCell::new(HashMap::<String, String>::new());

    let find_url = |rid: &str| -> String {
        if let Some(arr) = hyperlinks_json.as_array() {
            for item in arr {
                if let Some(item_arr) = item.as_array()
                    && item_arr.len() >= 2
                    && item_arr[0].as_str() == Some(rid)
                {
                    return item_arr[1].as_str().unwrap_or("").to_string();
                }
            }
        }
        String::new()
    };

    let find_image = |rid: &str| -> Option<String> {
        for img in &doc.images {
            if img.0 == rid {
                let zip_path = std::path::Path::new(&img.1);
                let original_file_name = zip_path.file_name()
                    .and_then(|f| f.to_str())
                    .unwrap_or("image.png");

                let file_name = if let Some(stem) = source_docx_stem.filter(|s| !s.is_empty()) {
                    if let Some(existing) = image_name_map.borrow().get(rid) {
                        existing.clone()
                    } else {
                        let ext = std::path::Path::new(original_file_name)
                            .extension()
                            .and_then(|e| e.to_str())
                            .filter(|e| !e.is_empty())
                            .unwrap_or("png");
                        let current = image_counter.get();
                        image_counter.set(current + 1);
                        let generated = format!("{}_image{:03}.{}", stem, current, ext);
                        image_name_map
                            .borrow_mut()
                            .insert(rid.to_string(), generated.clone());
                        generated
                    }
                } else {
                    original_file_name.to_string()
                };

                if let Some(dir) = output_dir {
                    if let Err(e) = std::fs::create_dir_all(dir) {
                        eprintln!("media ディレクトリの作成に失敗しました: {}", e);
                    }
                    let dest_path = dir.join(&file_name);
                    if let Err(e) = std::fs::write(&dest_path, &img.2.0) {
                        eprintln!("抽出画像ファイルの書き込みに失敗しました: {}", e);
                    }
                }
                return Some(format!("media/{}", file_name));
            }
        }
        None
    };

    let mut md = String::new();
    let children = &doc.document.children;
    let mut i = 0;

    while i < children.len() {
        match &children[i] {
            DocumentChild::Paragraph(p) => {
                let style = p.property.style.as_ref().map(|s| s.val.as_str());
                if let Some(s) = style
                    && s.starts_with("CodeBlock")
                {
                    let lang: &str = s.strip_prefix("CodeBlock-").unwrap_or_default();

                    let mut code_lines = Vec::new();
                    while i < children.len() {
                        if let DocumentChild::Paragraph(ref next_p) = children[i] {
                            let next_style = next_p.property.style.as_ref().map(|s| s.val.as_str());
                            if let Some(ns) = next_style
                                && ns.starts_with("CodeBlock")
                            {
                                code_lines.push(paragraph_raw_text(next_p));
                                i += 1;
                                continue;
                            }
                        }
                        break;
                    }

                    md.push_str("```");
                    md.push_str(lang);
                    md.push('\n');
                    md.push_str(&code_lines.join("\n"));
                    md.push_str("\n```\n\n");
                    continue;
                }
                
                md.push_str(&paragraph_to_md(p, &find_url, &find_image));
                i += 1;
            }
            DocumentChild::Table(t) => {
                md.push_str(&table_to_md(t, &find_url, &find_image));
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    Ok(md)
}

fn paragraph_raw_text(p: &Paragraph) -> String {
    let mut text = String::new();
    for child in &p.children {
        if let ParagraphChild::Run(r) = child {
            for run_child in &r.children {
                match run_child {
                    RunChild::Text(t) => {
                        text.push_str(&t.text);
                    }
                    RunChild::Tab(_) => {
                        text.push('\t');
                    }
                    RunChild::Break(_) => {
                        text.push('\n');
                    }
                    _ => {}
                }
            }
        }
    }
    if text == " " {
        String::new()
    } else {
        text
    }
}

fn paragraph_to_md<F, FI>(p: &Paragraph, find_url: &F, find_image: &FI) -> String
where
    F: Fn(&str) -> String,
    FI: Fn(&str) -> Option<String>,
{
    let mut md = String::new();
    
    // Check heading style
    let style = p.property.style.as_ref().map(|s| s.val.as_str());

    let heading_prefix = match style {
        Some("Heading1") => "# ",
        Some("Heading2") => "## ",
        Some("Heading3") => "### ",
        Some("Heading4") => "#### ",
        Some("Heading5") => "##### ",
        Some("Heading6") => "###### ",
        _ => "",
    };
    md.push_str(heading_prefix);
    let is_heading = !heading_prefix.is_empty();

    // Check list item numbering
    let list_prefix = if let Some(ref num_prop) = p.property.numbering_property {
        let level = num_prop.level.as_ref().map(|l| l.val).unwrap_or(0);
        let id = num_prop.id.as_ref().map(|i| i.id).unwrap_or(1);
        let indent = "  ".repeat(level);
        if id == 2 {
            format!("{}1. ", indent)
        } else {
            format!("{}* ", indent)
        }
    } else {
        String::new()
    };
    md.push_str(&list_prefix);

    // Check blockquote left indent
    let is_blockquote = if let Some(ref ind) = p.property.indent {
        ind.start.unwrap_or(0) >= 720
    } else {
        false
    };

    let mut body = String::new();
    for child in &p.children {
        body.push_str(&paragraph_child_to_md(child, find_url, find_image, is_blockquote, is_heading));
    }

    if is_blockquote {
        let mut formatted_body = String::new();
        formatted_body.push_str("> ");
        let lines: Vec<&str> = body.split('\n').collect();
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                if i == lines.len() - 1 && line.is_empty() {
                    break;
                }
                formatted_body.push_str("\n> ");
            }
            formatted_body.push_str(line);
        }
        body = formatted_body;
    }

    md.push_str(&body);
    
    if list_prefix.is_empty() {
        md.push('\n');
    }
    md.push('\n');
    md
}

fn paragraph_child_to_md<F, FI>(child: &ParagraphChild, find_url: &F, find_image: &FI, is_blockquote: bool, is_heading: bool) -> String
where
    F: Fn(&str) -> String,
    FI: Fn(&str) -> Option<String>,
{
    match child {
        ParagraphChild::Run(r) => run_to_md(r, is_blockquote, is_heading, find_image),
        ParagraphChild::Hyperlink(hl) => {
            let hl_json = serde_json::to_value(hl).unwrap_or(serde_json::Value::Null);
            let rid = hl_json["rid"].as_str().unwrap_or("");
            let url = find_url(rid);
            let text = hl.children.iter()
                .map(|c| paragraph_child_to_md(c, find_url, find_image, is_blockquote, is_heading))
                .collect::<Vec<String>>()
                .join("");
            format!("[{}]({})", text, url)
        }
        _ => String::new(),
    }
}

fn run_to_md<FI>(r: &Run, is_blockquote: bool, is_heading: bool, find_image: &FI) -> String
where
    FI: Fn(&str) -> Option<String>,
{
    let mut text = String::new();
    for child in &r.children {
        match child {
            RunChild::Text(t) => {
                text.push_str(&t.text);
            }
            RunChild::Tab(_) => {
                text.push('\t');
            }
            RunChild::Break(_) => {
                text.push('\n');
            }
            RunChild::Drawing(drawing) => {
                let drawing_json = serde_json::to_value(drawing).unwrap_or(serde_json::Value::Null);
                if drawing_json["type"].as_str() == Some("pic")
                    && let Some(rid) = drawing_json["data"]["id"].as_str()
                    && let Some(img_path) = find_image(rid)
                {
                    text.push_str(&format!("![image]({})", img_path));
                }
            }
            _ => {}
        }
    }

    let is_bold = r.run_property.bold.is_some() && !is_heading;
    let is_italic = r.run_property.italic.is_some() && !is_blockquote;
    let is_strike = r.run_property.strike.is_some();
    
    // Leverage JSON representation to bypass private fonts field access
    let run_prop_json = serde_json::to_value(&r.run_property).unwrap_or(serde_json::Value::Null);
    let is_code = run_prop_json["fonts"]["ascii"].as_str() == Some("Courier New");

    if is_code {
        format!("`{}`", text)
    } else {
        let mut formatted = text;
        if is_bold {
            formatted = format!("**{}**", formatted);
        }
        if is_italic {
            formatted = format!("*{}*", formatted);
        }
        if is_strike {
            formatted = format!("~~{}~~", formatted);
        }
        formatted
    }
}

fn table_to_md<F, FI>(t: &Table, find_url: &F, find_image: &FI) -> String
where
    F: Fn(&str) -> String,
    FI: Fn(&str) -> Option<String>,
{
    let mut md = String::new();
    let mut is_first_row = true;

    for row_child in &t.rows {
        match row_child {
            TableChild::TableRow(r) => {
                let mut cells_text = Vec::new();
                for table_row_child in &r.cells {
                    match table_row_child {
                        TableRowChild::TableCell(cell) => {
                            let mut cell_text = String::new();
                            for cell_child in &cell.children {
                                if let TableCellContent::Paragraph(p) = cell_child {
                                    cell_text.push_str(paragraph_to_md(p, find_url, find_image).trim_end());
                                }
                            }
                            cells_text.push(cell_text);
                        }
                    }
                }

                // Format row to Markdown table format
                md.push_str("| ");
                md.push_str(&cells_text.join(" | "));
                md.push_str(" |\n");

                if is_first_row {
                    md.push('|');
                    for _ in 0..cells_text.len() {
                        md.push_str("---|");
                    }
                    md.push('\n');
                    is_first_row = false;
                }
            }
        }
    }

    md.push('\n');
    md
}
