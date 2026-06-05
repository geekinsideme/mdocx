use crate::converter::{md_to_docx, docx_to_md};

#[test]
fn test_roundtrip_formatting() {
    let md = "# Heading 1\n\nThis is a paragraph with **bold**, *italic*, and ~~strikethrough~~ text, along with some `inline code`.\n";
    let docx_bytes = md_to_docx(md).expect("MD to DOCX failed");
    let result_md = docx_to_md(&docx_bytes).expect("DOCX to MD failed");
    
    // Assert headings and key text exist in the output
    assert!(result_md.contains("# Heading 1"));
    assert!(result_md.contains("**bold**"));
    assert!(result_md.contains("*italic*"));
    assert!(result_md.contains("~~strikethrough~~"));
    assert!(result_md.contains("`inline code`"));
}

#[test]
fn test_roundtrip_blockquote() {
    let md = "> This is a blockquote.\n";
    let docx_bytes = md_to_docx(md).expect("MD to DOCX failed");
    let result_md = docx_to_md(&docx_bytes).expect("DOCX to MD failed");
    assert!(result_md.contains("> This is a blockquote."));
}

#[test]
fn test_roundtrip_lists() {
    let md = "* Bullet item 1\n* Bullet item 2\n\n1. Numbered item 1\n1. Numbered item 2\n";
    let docx_bytes = md_to_docx(md).expect("MD to DOCX failed");
    let result_md = docx_to_md(&docx_bytes).expect("DOCX to MD failed");
    
    assert!(result_md.contains("* Bullet item 1"));
    assert!(result_md.contains("* Bullet item 2"));
    assert!(result_md.contains("1. Numbered item 1"));
    assert!(result_md.contains("1. Numbered item 2"));
}

#[test]
fn test_roundtrip_hyperlink() {
    let md = "Check out [Google](https://google.com) for details.\n";
    let docx_bytes = md_to_docx(md).expect("MD to DOCX failed");
    let result_md = docx_to_md(&docx_bytes).expect("DOCX to MD failed");
    assert!(result_md.contains("[Google](https://google.com)"));
}

#[test]
fn test_roundtrip_table() {
    let md = "| Header 1 | Header 2 |\n|---|---|\n| Cell 1 | Cell 2 |\n\n";
    let docx_bytes = md_to_docx(md).expect("MD to DOCX failed");
    let doc = docx_rs::read_docx(&docx_bytes).expect("Read DOCX failed");
    println!("FULL DOCX JSON FOR TABLE:\n{}", serde_json::to_string_pretty(&doc).unwrap_or_default());
    let result_md = docx_to_md(&docx_bytes).expect("DOCX to MD failed");
    println!("Table result:\n{:?}", result_md);
    assert!(result_md.contains("| Header 1 | Header 2 |"));
    assert!(result_md.contains("| Cell 1 | Cell 2 |"));
}

#[test]
fn test_roundtrip_image_link() {
    let md = "Here is an [image link](https://example.com/logo.png).\n";
    let docx_bytes = md_to_docx(md).expect("MD to DOCX failed");
    let result_md = docx_to_md(&docx_bytes).expect("DOCX to MD failed");
    assert!(result_md.contains("[image link](https://example.com/logo.png)"));
}
