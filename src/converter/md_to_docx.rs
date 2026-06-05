use docx_rs::*;
use pulldown_cmark::{Parser, Event, Tag, TagEnd, HeadingLevel, Options};
use std::io::Read;

struct ListState {
    is_ordered: bool,
    level: usize,
}

struct TableState {
    rows: Vec<TableRow>,
    current_row_cells: Vec<TableCell>,
    current_cell_paragraphs: Vec<Paragraph>,
}

pub fn md_to_docx(md_content: &str) -> Result<Vec<u8>, anyhow::Error> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(md_content, options);

    let mut docx = Docx::new();

    // Register Abstract Numbering for Lists
    // Unordered list (bullet points) - ID 1
    let mut bullet_num = AbstractNumbering::new(1);
    for lvl in 0..9 {
        bullet_num = bullet_num.add_level(
            Level::new(
                lvl as usize,
                Start::new(1),
                NumberFormat::new("bullet"),
                LevelText::new("•"),
                LevelJc::new("left"),
            )
        );
    }
    docx = docx.add_abstract_numbering(bullet_num).add_numbering(Numbering::new(1, 1));

    // Ordered list (decimal numbers) - ID 2
    let mut decimal_num = AbstractNumbering::new(2);
    for lvl in 0..9 {
        let mut level_text = String::new();
        for i in 0..=lvl {
            level_text.push_str(&format!("%{}", i + 1));
            if i < lvl {
                level_text.push('.');
            }
        }
        level_text.push('.');

        decimal_num = decimal_num.add_level(
            Level::new(
                lvl as usize,
                Start::new(1),
                NumberFormat::new("decimal"),
                LevelText::new(level_text),
                LevelJc::new("left"),
            )
        );
    }
    docx = docx.add_abstract_numbering(decimal_num).add_numbering(Numbering::new(2, 2));

    // State trackers
    let mut current_paragraph: Option<Paragraph> = None;
    let mut lists: Vec<ListState> = Vec::new();
    let mut in_list_item = false;
    let mut is_first_p_in_item = false;
    
    let mut bold = false;
    let mut italic = false;
    let mut strike = false;
    let mut link_url: Option<String> = None;
    let mut image_url: Option<String> = None;
    let mut image_alt = String::new();
    let mut in_code_block = false;
    let mut in_blockquote = false;
    let mut code_block_style: Option<String> = None;
    let mut heading_level: Option<HeadingLevel> = None;

    let mut table_state: Option<TableState> = None;

    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Heading { level, .. } => {
                    let style_name = match level {
                        HeadingLevel::H1 => "Heading1",
                        HeadingLevel::H2 => "Heading2",
                        HeadingLevel::H3 => "Heading3",
                        HeadingLevel::H4 => "Heading4",
                        HeadingLevel::H5 => "Heading5",
                        HeadingLevel::H6 => "Heading6",
                    };
                    if let Some(p) = current_paragraph.take() {
                        if let Some(ref mut ts) = table_state {
                            ts.current_cell_paragraphs.push(p);
                        } else {
                            docx = docx.add_paragraph(p);
                        }
                    }
                    let mut p = Paragraph::new().style(style_name);
                    let (before, after) = match level {
                        HeadingLevel::H1 => (240, 120),
                        HeadingLevel::H2 => (180, 80),
                        HeadingLevel::H3 => (140, 60),
                        _ => (120, 60),
                    };
                    p = p.line_spacing(LineSpacing::new().before(before).after(after));
                    current_paragraph = Some(p);
                    heading_level = Some(level);
                }
                Tag::Paragraph => {
                    if in_list_item && is_first_p_in_item {
                        is_first_p_in_item = false;
                    } else {
                        if let Some(p) = current_paragraph.take() {
                            if let Some(ref mut ts) = table_state {
                                ts.current_cell_paragraphs.push(p);
                            } else {
                                docx = docx.add_paragraph(p);
                            }
                        }
                        let mut p = Paragraph::new();
                        if in_list_item {
                            if let Some(list) = lists.last() {
                                p = p.indent(Some(((list.level + 1) * 360) as i32), None, None, None);
                            }
                        } else if in_blockquote {
                            p = p.indent(Some(720), None, None, None);
                        }
                        current_paragraph = Some(p);
                    }
                }
                Tag::BlockQuote => {
                    in_blockquote = true;
                }
                Tag::CodeBlock(kind) => {
                    in_code_block = true;
                    if let Some(p) = current_paragraph.take() {
                        if let Some(ref mut ts) = table_state {
                            ts.current_cell_paragraphs.push(p);
                        } else {
                            docx = docx.add_paragraph(p);
                        }
                    }
                    let style_name = match kind {
                        pulldown_cmark::CodeBlockKind::Fenced(lang) => {
                            if lang.is_empty() {
                                "CodeBlock".to_string()
                            } else {
                                format!("CodeBlock-{}", lang)
                            }
                        }
                        pulldown_cmark::CodeBlockKind::Indented => "CodeBlock".to_string(),
                    };
                    code_block_style = Some(style_name);
                    current_paragraph = None;
                }
                Tag::List(start_number) => {
                    lists.push(ListState {
                        is_ordered: start_number.is_some(),
                        level: lists.len(),
                    });
                }
                Tag::Item => {
                    in_list_item = true;
                    is_first_p_in_item = true;
                    
                    if let Some(p) = current_paragraph.take() {
                        if let Some(ref mut ts) = table_state {
                            ts.current_cell_paragraphs.push(p);
                        } else {
                            docx = docx.add_paragraph(p);
                        }
                    }

                    let mut p = Paragraph::new();
                    if let Some(list) = lists.last() {
                        p = p.numbering(
                            NumberingId::new(if list.is_ordered { 2 } else { 1 }),
                            IndentLevel::new(list.level),
                        );
                    }
                    current_paragraph = Some(p);
                }
                Tag::Table(_) => {
                    table_state = Some(TableState {
                        rows: Vec::new(),
                        current_row_cells: Vec::new(),
                        current_cell_paragraphs: Vec::new(),
                    });
                }
                Tag::TableHead => {
                    if let Some(ref mut ts) = table_state {
                        ts.current_row_cells.clear();
                    }
                }
                Tag::TableRow => {
                    if let Some(ref mut ts) = table_state {
                        ts.current_row_cells.clear();
                    }
                }
                Tag::TableCell => {
                    if let Some(ref mut ts) = table_state {
                        ts.current_cell_paragraphs.clear();
                    }
                }
                Tag::Emphasis => {
                    italic = true;
                }
                Tag::Strong => {
                    bold = true;
                }
                Tag::Strikethrough => {
                    strike = true;
                }
                Tag::Link { dest_url, .. } => {
                    link_url = Some(dest_url.to_string());
                }
                Tag::Image { dest_url, .. } => {
                    image_url = Some(dest_url.to_string());
                    image_alt.clear();
                }
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Heading(_) => {
                    heading_level = None;
                    if let Some(p) = current_paragraph.take() {
                        if let Some(ref mut ts) = table_state {
                            ts.current_cell_paragraphs.push(p);
                        } else {
                            docx = docx.add_paragraph(p);
                        }
                    }
                }
                TagEnd::Paragraph => {
                    if let Some(p) = current_paragraph.take() {
                        if let Some(ref mut ts) = table_state {
                            ts.current_cell_paragraphs.push(p);
                        } else {
                            docx = docx.add_paragraph(p);
                        }
                    }
                }
                TagEnd::BlockQuote => {
                    in_blockquote = false;
                }
                TagEnd::CodeBlock => {
                    in_code_block = false;
                    code_block_style = None;
                }
                TagEnd::List(_) => {
                    lists.pop();
                }
                TagEnd::Item => {
                    in_list_item = false;
                    is_first_p_in_item = false;
                    if let Some(p) = current_paragraph.take() {
                        if let Some(ref mut ts) = table_state {
                            ts.current_cell_paragraphs.push(p);
                        } else {
                            docx = docx.add_paragraph(p);
                        }
                    }
                }
                TagEnd::TableCell => {
                    if let Some(ref mut ts) = table_state {
                        if let Some(p) = current_paragraph.take() {
                            ts.current_cell_paragraphs.push(p);
                        }
                        let mut cell = TableCell::new();
                        if ts.current_cell_paragraphs.is_empty() {
                            cell = cell.add_paragraph(Paragraph::new());
                        } else {
                            for p in ts.current_cell_paragraphs.drain(..) {
                                cell = cell.add_paragraph(p);
                            }
                        }
                        ts.current_row_cells.push(cell);
                    }
                }
                TagEnd::TableHead => {
                    if let Some(ref mut ts) = table_state {
                        let row = TableRow::new(ts.current_row_cells.drain(..).collect());
                        ts.rows.push(row);
                    }
                }
                TagEnd::TableRow => {
                    if let Some(ref mut ts) = table_state {
                        let row = TableRow::new(ts.current_row_cells.drain(..).collect());
                        ts.rows.push(row);
                    }
                }
                TagEnd::Table => {
                    if let Some(mut ts) = table_state.take() {
                        let table = Table::new(ts.rows.drain(..).collect());
                        docx = docx.add_table(table);
                    }
                }
                TagEnd::Emphasis => {
                    italic = false;
                }
                TagEnd::Strong => {
                    bold = false;
                }
                TagEnd::Strikethrough => {
                    strike = false;
                }
                TagEnd::Link => {
                    link_url = None;
                }
                TagEnd::Image => {
                    if let Some(url) = image_url.take() {
                        let alt = std::mem::take(&mut image_alt);
                        match fetch_image_bytes(&url) {
                            Ok(bytes) => {
                                let pic = Pic::new(&bytes).size(3000000, 2000000);
                                let run = Run::new().add_image(pic);
                                let p = current_paragraph.take().unwrap_or_else(Paragraph::new);
                                current_paragraph = Some(p.add_run(run));
                            }
                            Err(_) => {
                                // フォールバック：マークダウン形式のテキストとして埋め込む
                                let fallback_text = format!("![{}]({})", alt, url);
                                let run = Run::new().add_text(fallback_text);
                                let p = current_paragraph.take().unwrap_or_else(Paragraph::new);
                                current_paragraph = Some(p.add_run(run));
                            }
                        }
                    }
                }
                _ => {}
            },
            Event::Text(text) => {
                if image_url.is_some() {
                    image_alt.push_str(&text);
                } else if in_code_block {
                    let style_name = code_block_style.as_deref().unwrap_or("CodeBlock");
                    let lines: Vec<&str> = text.split('\n').collect();
                    let total_lines = if lines.last().map_or(false, |l| l.is_empty()) {
                        lines.len() - 1
                    } else {
                        lines.len()
                    };

                    for (i, line) in lines.iter().enumerate() {
                        if i == total_lines {
                            break;
                        }

                        let is_first = i == 0;
                        let is_last = i == total_lines - 1;

                        let mut p = Paragraph::new().style(style_name);

                        // Line spacing & spacing before/after
                        let mut ls = LineSpacing::new().line(240).line_rule(LineSpacingType::Auto);
                        if is_first {
                            ls = ls.before(120);
                        } else {
                            ls = ls.before(0);
                        }
                        if is_last {
                            ls = ls.after(120);
                        } else {
                            ls = ls.after(0);
                        }
                        p = p.line_spacing(ls);

                        // Indent
                        let left_indent = if in_blockquote { 1080 } else { 360 };
                        p = p.indent(Some(left_indent), None, None, None);

                        // Run with text & shading
                        let text_val = if line.is_empty() { " " } else { line };
                        let run = Run::new()
                            .add_text(text_val.to_string())
                            .fonts(RunFonts::new().ascii("Courier New").east_asia("MS Gothic"))
                            .shading(Shading::new().fill("F4F5F7"));

                        p = p.add_run(run);
                        if let Some(ref mut ts) = table_state {
                            ts.current_cell_paragraphs.push(p);
                        } else {
                            docx = docx.add_paragraph(p);
                        }
                    }
                    current_paragraph = None;
                } else {
                    let mut run = Run::new().add_text(text.to_string());
                    if let Some(lvl) = heading_level {
                        let size = match lvl {
                            HeadingLevel::H1 => 40,
                            HeadingLevel::H2 => 32,
                            HeadingLevel::H3 => 28,
                            HeadingLevel::H4 => 24,
                            HeadingLevel::H5 => 22,
                            HeadingLevel::H6 => 20,
                        };
                        run = run.bold().size(size).color("2F3542");
                    }
                    if bold { run = run.bold(); }
                    if italic { run = run.italic(); }
                    if strike { run = run.strike(); }
                    if in_blockquote { run = run.italic(); }

                    let p = current_paragraph.take().unwrap_or_else(Paragraph::new);
                    if let Some(ref url) = link_url {
                        let hl_run = run.color("0563C1").underline("single");
                        let hl = Hyperlink::new(url, HyperlinkType::External).add_run(hl_run);
                        current_paragraph = Some(p.add_hyperlink(hl));
                    } else {
                        current_paragraph = Some(p.add_run(run));
                    }
                }
            }
            Event::Code(code) => {
                if image_url.is_some() {
                    image_alt.push_str(&format!("`{}`", code));
                } else {
                    let mut run = Run::new()
                        .add_text(code.to_string())
                        .fonts(RunFonts::new().ascii("Courier New"))
                        .highlight("lightGray");

                    if let Some(lvl) = heading_level {
                        let size = match lvl {
                            HeadingLevel::H1 => 40,
                            HeadingLevel::H2 => 32,
                            HeadingLevel::H3 => 28,
                            HeadingLevel::H4 => 24,
                            HeadingLevel::H5 => 22,
                            HeadingLevel::H6 => 20,
                        };
                        run = run.bold().size(size).color("2F3542");
                    }

                    if bold { run = run.bold(); }
                    if italic { run = run.italic(); }
                    if strike { run = run.strike(); }

                    let p = current_paragraph.take().unwrap_or_else(Paragraph::new);
                    if let Some(ref url) = link_url {
                        let hl_run = run.color("0563C1").underline("single");
                        let hl = Hyperlink::new(url, HyperlinkType::External).add_run(hl_run);
                        current_paragraph = Some(p.add_hyperlink(hl));
                    } else {
                        current_paragraph = Some(p.add_run(run));
                    }
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if image_url.is_none() {
                    let p = current_paragraph.take().unwrap_or_else(Paragraph::new);
                    current_paragraph = Some(p.add_run(Run::new().add_break(BreakType::TextWrapping)));
                }
            }
            Event::Rule => {
                let p = Paragraph::new().add_run(Run::new().add_text("---"));
                if let Some(ref mut ts) = table_state {
                    ts.current_cell_paragraphs.push(p);
                } else {
                    docx = docx.add_paragraph(p);
                }
            }
            _ => {}
        }
    }

    if let Some(p) = current_paragraph {
        if let Some(ref mut ts) = table_state {
            ts.current_cell_paragraphs.push(p);
        } else {
            docx = docx.add_paragraph(p);
        }
    }

    let mut buffer = std::io::Cursor::new(Vec::new());
    docx.build().pack(&mut buffer)?;
    Ok(buffer.into_inner())
}

fn fetch_image_bytes(url: &str) -> Result<Vec<u8>, anyhow::Error> {
    if url.starts_with("http://") || url.starts_with("https://") {
        let resp = ureq::get(url).call()?;
        let mut bytes = Vec::new();
        resp.into_reader().read_to_end(&mut bytes)?;
        Ok(bytes)
    } else {
        let bytes = std::fs::read(url)?;
        Ok(bytes)
    }
}
