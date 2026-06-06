use crate::converter::{md_to_docx, docx_to_md};

#[test]
fn test_roundtrip_formatting() {
    let md = "# Heading 1\n\nThis is a paragraph with **bold**, *italic*, and ~~strikethrough~~ text, along with some `inline code`.\n";
    let docx_bytes = md_to_docx(md).expect("MD to DOCX failed");
    let result_md = docx_to_md(&docx_bytes, None, None).expect("DOCX to MD failed");
    
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
    let result_md = docx_to_md(&docx_bytes, None, None).expect("DOCX to MD failed");
    assert!(result_md.contains("> This is a blockquote."));
}

#[test]
fn test_roundtrip_lists() {
    let md = "* Bullet item 1\n* Bullet item 2\n\n1. Numbered item 1\n1. Numbered item 2\n";
    let docx_bytes = md_to_docx(md).expect("MD to DOCX failed");
    let result_md = docx_to_md(&docx_bytes, None, None).expect("DOCX to MD failed");
    
    assert!(result_md.contains("* Bullet item 1"));
    assert!(result_md.contains("* Bullet item 2"));
    assert!(result_md.contains("1. Numbered item 1"));
    assert!(result_md.contains("1. Numbered item 2"));
}

#[test]
fn test_roundtrip_hyperlink() {
    let md = "Check out [Google](https://google.com) for details.\n";
    let docx_bytes = md_to_docx(md).expect("MD to DOCX failed");
    let result_md = docx_to_md(&docx_bytes, None, None).expect("DOCX to MD failed");
    assert!(result_md.contains("[Google](https://google.com)"));
}

#[test]
fn test_roundtrip_table() {
    let md = "| Header 1 | Header 2 |\n|---|---|\n| Cell 1 | Cell 2 |\n\n";
    let docx_bytes = md_to_docx(md).expect("MD to DOCX failed");
    let doc = docx_rs::read_docx(&docx_bytes).expect("Read DOCX failed");
    println!("FULL DOCX JSON FOR TABLE:\n{}", serde_json::to_string_pretty(&doc).unwrap_or_default());
    let result_md = docx_to_md(&docx_bytes, None, None).expect("DOCX to MD failed");
    println!("Table result:\n{:?}", result_md);
    assert!(result_md.contains("| Header 1 | Header 2 |"));
    assert!(result_md.contains("| Cell 1 | Cell 2 |"));
}

#[test]
fn test_roundtrip_image_link() {
    let md = "Here is an [image link](https://example.com/logo.png).\n";
    let docx_bytes = md_to_docx(md).expect("MD to DOCX failed");
    let result_md = docx_to_md(&docx_bytes, None, None).expect("DOCX to MD failed");
    assert!(result_md.contains("[image link](https://example.com/logo.png)"));
}

#[test]
fn test_image_local_and_fallback() {
    use std::io::Write;
    let temp_dir = std::env::temp_dir();
    let temp_file_path = temp_dir.join("mdocx_test_dummy_image.png");
    {
        let mut file = std::fs::File::create(&temp_file_path).expect("Failed to create temp image file");
        let png_bytes = [
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8,
            6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 10, 73, 68, 65, 84, 120, 156, 99, 0, 1, 0, 0, 5,
            0, 1, 13, 10, 45, 180, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130
        ];
        file.write_all(&png_bytes).expect("Failed to write to temp image file");
    }

    // Convert with local image
    let md = format!("Here is a local image: ![Local Dummy Image]({})\n", temp_file_path.to_string_lossy().replace('\\', "/"));
    let docx_bytes = md_to_docx(&md).expect("MD to DOCX with local image failed");
    
    let doc = docx_rs::read_docx(&docx_bytes).expect("Read DOCX failed");
    let doc_json = serde_json::to_string(&doc).unwrap();
    assert!(doc_json.contains("drawing") || doc_json.contains("Drawing") || doc_json.contains("pic"));

    let _ = std::fs::remove_file(&temp_file_path);

    // Convert with failed fetch (fallback to markdown syntax)
    let md_fallback = "Missing image: ![Missing Alt](non_existent_file.png)\n";
    let docx_bytes_fallback = md_to_docx(md_fallback).expect("MD to DOCX fallback failed");
    let result_md = docx_to_md(&docx_bytes_fallback, None, None).expect("DOCX to MD fallback failed");
    assert!(result_md.contains("![Missing Alt](non_existent_file.png)"));
}

#[test]
fn test_docx_to_md_image_extraction() {
    use std::io::Write;
    let temp_dir = std::env::temp_dir();
    let temp_file_path = temp_dir.join("mdocx_test_dummy_image2.png");
    {
        let mut file = std::fs::File::create(&temp_file_path).expect("Failed to create temp image file");
        let png_bytes = [
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8,
            6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 10, 73, 68, 65, 84, 120, 156, 99, 0, 1, 0, 0, 5,
            0, 1, 13, 10, 45, 180, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130
        ];
        file.write_all(&png_bytes).expect("Failed to write to temp image file");
    }

    let md = format!("Here is a local image: ![Local Dummy Image]({})\n", temp_file_path.to_string_lossy().replace('\\', "/"));
    let docx_bytes = md_to_docx(&md).expect("MD to DOCX failed");

    let output_media_dir = temp_dir.join("mdocx_test_output_media");
    let _ = std::fs::remove_dir_all(&output_media_dir);

    let result_md = docx_to_md(&docx_bytes, Some(&output_media_dir), Some("abc")).expect("DOCX to MD failed");

    assert!(result_md.contains("![image](media/abc_image001.png)"));
    
    let expected_extracted_path = output_media_dir.join("abc_image001.png");
    assert!(expected_extracted_path.exists(), "Extracted image file does not exist!");
    
    let _ = std::fs::remove_file(&temp_file_path);
    let _ = std::fs::remove_file(&expected_extracted_path);
    let _ = std::fs::remove_dir(&output_media_dir);
}
