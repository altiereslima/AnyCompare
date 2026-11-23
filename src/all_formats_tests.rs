// Comprehensive end-to-end file format matrix test suite.
// Included inside `#[cfg(test)] mod tests` in `src/main.rs`.

fn make_zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut zw = zip::ZipWriter::new(&mut cursor);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in entries {
            zw.start_file(*name, opts).unwrap();
            zw.write_all(data).unwrap();
        }
        zw.finish().unwrap();
    }
    cursor.into_inner()
}

fn make_tar_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, data) in entries {
        let mut header = [0u8; 512];
        let nb = name.as_bytes();
        header[..nb.len()].copy_from_slice(nb);
        header[100..108].copy_from_slice(b"0000644\0");
        header[108..116].copy_from_slice(b"0000000\0");
        header[116..124].copy_from_slice(b"0000000\0");
        let size_oct = format!("{:011o}\0", data.len());
        header[124..136].copy_from_slice(size_oct.as_bytes());
        header[136..148].copy_from_slice(b"14500000000\0");
        header[148..156].copy_from_slice(b"        ");
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        let csum: u32 = header.iter().map(|&b| b as u32).sum();
        let csum_oct = format!("{csum:06o}\0 ");
        header[148..156].copy_from_slice(csum_oct.as_bytes());
        out.extend_from_slice(&header);
        out.extend_from_slice(data);
        let rem = data.len() % 512;
        if rem != 0 {
            out.extend(std::iter::repeat_n(0u8, 512 - rem));
        }
    }
    out.extend(std::iter::repeat_n(0u8, 1024));
    out
}

fn make_minimal_pdf(line1: &str, line2: &str) -> Vec<u8> {
    use std::io::Write;
    let content = format!(
        "BT /F1 12 Tf 72 720 Td ({}) Tj 0 -18 Td ({}) Tj ET",
        line1.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)"),
        line2.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)"),
    );
    let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(content.as_bytes()).unwrap();
    let zcontent = enc.finish().unwrap();

    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.4\n");
    let objs: Vec<Vec<u8>> = vec![
        b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".to_vec(),
        b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n".to_vec(),
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>\nendobj\n".to_vec(),
        b"4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>\nendobj\n".to_vec(),
        [
            format!("5 0 obj\n<< /Filter /FlateDecode /Length {} >>\nstream\n", zcontent.len()).into_bytes(),
            zcontent,
            b"\nendstream\nendobj\n".to_vec(),
        ].concat(),
    ];
    let mut offsets = Vec::new();
    for o in &objs {
        offsets.push(pdf.len());
        pdf.extend_from_slice(o);
    }
    let xref_pos = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for off in offsets {
        pdf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n",
            objs.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

fn make_minimal_dbf(rows: &[(&str, &str)]) -> Vec<u8> {
    // dBase III table with 2 character columns: ID (10 bytes), NAME (20 bytes)
    let mut dbf = Vec::new();
    let num_records = rows.len() as u32;
    let header_len: u16 = 32 + 32 * 2 + 1;
    let record_len: u16 = 1 + 10 + 20;
    dbf.push(0x03); // dBase III
    dbf.extend_from_slice(&[26, 11, 21]); // YY MM DD
    dbf.extend_from_slice(&num_records.to_le_bytes());
    dbf.extend_from_slice(&header_len.to_le_bytes());
    dbf.extend_from_slice(&record_len.to_le_bytes());
    dbf.extend_from_slice(&[0u8; 20]); // reserved

    for (name, len) in [("ID", 10u8), ("NAME", 20u8)] {
        let mut fd = [0u8; 32];
        fd[..name.len()].copy_from_slice(name.as_bytes());
        fd[11] = b'C';
        fd[16] = len;
        dbf.extend_from_slice(&fd);
    }
    dbf.push(0x0D); // header terminator
    for (id, name) in rows {
        dbf.push(b' '); // active record
        let mut id_buf = [b' '; 10];
        let ib = id.as_bytes();
        id_buf[..ib.len().min(10)].copy_from_slice(&ib[..ib.len().min(10)]);
        dbf.extend_from_slice(&id_buf);
        let mut nm_buf = [b' '; 20];
        let nb = name.as_bytes();
        nm_buf[..nb.len().min(20)].copy_from_slice(&nb[..nb.len().min(20)]);
        dbf.extend_from_slice(&nm_buf);
    }
    dbf.push(0x1A); // EOF
    dbf
}

fn make_minimal_wav(samples: &[i16]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let riff_len = 36 + data_len;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&riff_len.to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // PCM fmt chunk size
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&44100u32.to_le_bytes()); // sample rate
    wav.extend_from_slice(&88200u32.to_le_bytes()); // byte rate
    wav.extend_from_slice(&2u16.to_le_bytes()); // block align
    wav.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        wav.extend_from_slice(&s.to_le_bytes());
    }
    wav
}

fn make_minimal_mp4(brand: &[u8; 4], title: &str) -> Vec<u8> {
    fn atom(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let len = (8 + payload.len()) as u32;
        let mut out = Vec::with_capacity(len as usize);
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(payload);
        out
    }
    let mut ftyp_payload = Vec::new();
    ftyp_payload.extend_from_slice(brand);
    ftyp_payload.extend_from_slice(&0u32.to_be_bytes());
    ftyp_payload.extend_from_slice(b"isom");
    let ftyp = atom(b"ftyp", &ftyp_payload);

    let mut data_payload = Vec::new();
    data_payload.extend_from_slice(&1u32.to_be_bytes()); // UTF-8 type
    data_payload.extend_from_slice(&0u32.to_be_bytes()); // locale
    data_payload.extend_from_slice(title.as_bytes());
    let data_atom = atom(b"data", &data_payload);
    let nam = atom(b"\xA9nam", &data_atom);
    let ilst = atom(b"ilst", &nam);
    let mut meta_payload = vec![0u8; 4]; // fullbox version/flags
    meta_payload.extend_from_slice(&ilst);
    let meta = atom(b"meta", &meta_payload);
    let udta = atom(b"udta", &meta);
    let moov = atom(b"moov", &udta);

    let mut out = ftyp;
    out.extend_from_slice(&moov);
    out
}

fn decode_hex_bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

fn make_test_png(color: [u8; 4]) -> Vec<u8> {
    let mut img = image::RgbaImage::new(8, 8);
    for p in img.pixels_mut() {
        *p = image::Rgba(color);
    }
    let mut buf = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut buf, image::ImageFormat::Png)
        .unwrap();
    buf.into_inner()
}

fn make_test_ico(color: [u8; 4]) -> Vec<u8> {
    let png = make_test_png(color);
    let mut ico = Vec::with_capacity(22 + png.len());
    ico.extend_from_slice(&[0, 0, 1, 0, 1, 0]); // ICONDIR: 1 icon
    ico.extend_from_slice(&[8, 8, 0, 0, 1, 0, 32, 0]); // 8x8 32bpp
    ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
    ico.extend_from_slice(&22u32.to_le_bytes()); // offset to PNG payload
    ico.extend_from_slice(&png);
    ico
}

#[test]
fn comprehensive_all_file_formats_routing_loading_and_diff_matrix() {
    use std::io::Write;
    let base = std::env::temp_dir().join(format!(
        "anycompare-all-formats-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let left_dir = base.join("left");
    let right_dir = base.join("right");
    std::fs::create_dir_all(&left_dir).unwrap();
    std::fs::create_dir_all(&right_dir).unwrap();
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    // -----------------------------------------------------------------------
    // 1. Plain & structured text formats (6 formats) -> Session::Text editable
    // -----------------------------------------------------------------------
    let text_cases: &[(&str, &str, &str)] = &[
        ("plain.txt", "Olá linha 1\nlinha 2\n", "Olá linha 1\nlinha 2 alterada\n"),
        ("notes.md", "# Título\n- item A\n", "# Título\n- item B\n"),
        ("config.json", "{\"a\": 1, \"b\": \"ok\"}\n", "{\"a\": 2, \"b\": \"ok\"}\n"),
        ("data.xml", "<root><item>um</item></root>\n", "<root><item>dois</item></root>\n"),
        ("page.html", "<html><body><h1>Título</h1><p>Texto simples</p></body></html>\n", "<html><body><h1>Título</h1><p>Texto modificado</p></body></html>\n"),
        ("code.rs", "fn main() { println!(\"A\"); }\n", "fn main() { println!(\"B\"); }\n"),
    ];
    for (name, l_txt, r_txt) in text_cases {
        let lp = left_dir.join(name);
        let rp = right_dir.join(name);
        std::fs::write(&lp, l_txt).unwrap();
        std::fs::write(&rp, r_txt).unwrap();
        assert_eq!(SessionKind::for_paths(&lp, &rp), SessionKind::Text, "routing {name}");
        let mut app = drop_app();
        let mut dialogs = ui::dialogs::TestDialogs::default();
        app.handle_dropped_paths(&[lp, rp], None, &mut dialogs);
        assert!(dialogs.errors.is_empty(), "{name}: {:?}", dialogs.errors);
        let Session::Text(screen) = &app.sessions[0] else {
            panic!("expected Session::Text for {name}");
        };
        assert!(!screen.left.is_converted(), "{name} must remain directly editable");
        assert_ne!(screen.left.text, screen.right.text, "{name} diff text");
    }

    // -----------------------------------------------------------------------
    // 2. Document-to-Text converted formats (8 formats) -> Session::Text read-only
    // -----------------------------------------------------------------------
    let docx_xml = |body: &str| {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>{body}</w:t></w:r></w:p></w:body></w:document>"#
        )
    };
    let odt_xml = |body: &str| {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><office:body><office:text><text:p>{body}</text:p></office:text></office:body></office:document-content>"#
        )
    };
    let pptx_xml = |body: &str| {
        format!(
            r#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>{body}</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#
        )
    };
    let epub_bytes = |body: &str| {
        let container = br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;
        let opf = br#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf"><manifest><item id="c1" href="ch1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>"#;
        let ch = format!("<html><body><p>{body}</p></body></html>");
        make_zip_bytes(&[
            ("META-INF/container.xml", container),
            ("OEBPS/content.opf", opf),
            ("OEBPS/ch1.xhtml", ch.as_bytes()),
        ])
    };
    let doc_cases: Vec<(&str, &str, Vec<u8>, Vec<u8>)> = vec![
        (
            "livro.pdf",
            "doc.format.pdf",
            make_minimal_pdf("Drama Cosmico", "Capitulo 1"),
            make_minimal_pdf("Drama Terreno", "Capitulo 1"),
        ),
        (
            "contrato.docx",
            "doc.format.word",
            make_zip_bytes(&[("word/document.xml", docx_xml("Cláusula A").as_bytes())]),
            make_zip_bytes(&[("word/document.xml", docx_xml("Cláusula B").as_bytes())]),
        ),
        (
            "legado.doc",
            "doc.format.word",
            br#"{\rtf1\ansi Documento antigo esquerda\par}"#.to_vec(),
            br#"{\rtf1\ansi Documento antigo direita\par}"#.to_vec(),
        ),
        (
            "nota.rtf",
            "doc.format.rtf",
            br#"{\rtf1\ansi\ansicpg1252 Texto com a\'e7\'e3o esquerda\par}"#.to_vec(),
            br#"{\rtf1\ansi\ansicpg1252 Texto com a\'e7\'e3o direita\par}"#.to_vec(),
        ),
        (
            "texto.odt",
            "doc.format.odt",
            make_zip_bytes(&[("content.xml", odt_xml("Parágrafo esquerdo").as_bytes())]),
            make_zip_bytes(&[("content.xml", odt_xml("Parágrafo direito").as_bytes())]),
        ),
        (
            "slides.pptx",
            "doc.format.pptx",
            make_zip_bytes(&[("ppt/slides/slide1.xml", pptx_xml("Slide Alpha").as_bytes())]),
            make_zip_bytes(&[("ppt/slides/slide1.xml", pptx_xml("Slide Beta").as_bytes())]),
        ),
        (
            "apresentacao.odp",
            "doc.format.odp",
            make_zip_bytes(&[("content.xml", odt_xml("Lâmina 1").as_bytes())]),
            make_zip_bytes(&[("content.xml", odt_xml("Lâmina 2").as_bytes())]),
        ),
        (
            "ebook.epub",
            "doc.format.epub",
            epub_bytes("Capítulo Um Esquerda"),
            epub_bytes("Capítulo Um Direita"),
        ),
    ];
    for (name, expected_fmt, l_bytes, r_bytes) in doc_cases {
        let lp = left_dir.join(name);
        let rp = right_dir.join(name);
        std::fs::write(&lp, &l_bytes).unwrap();
        std::fs::write(&rp, &r_bytes).unwrap();
        assert_eq!(SessionKind::for_paths(&lp, &rp), SessionKind::Text, "routing {name}");
        let mut app = drop_app();
        let mut dialogs = ui::dialogs::TestDialogs::default();
        app.handle_dropped_paths(&[lp.clone(), rp.clone()], None, &mut dialogs);
        assert!(dialogs.errors.is_empty(), "{name}: {:?}", dialogs.errors);
        let Session::Text(screen) = &mut app.sessions[0] else {
            panic!("expected Session::Text for converted {name}");
        };
        assert!(screen.left.is_converted(), "{name} must be marked converted/read-only");
        assert_eq!(screen.left.format_label(), expected_fmt, "{name} format_label");
        assert_ne!(screen.left.text, screen.right.text, "{name} must detect extracted text diff");
        // Verify original binary file CANNOT be overwritten in-place by save().
        assert!(
            screen.left.save(&lp).is_err(),
            "{name} converted document must refuse in-place save"
        );
        assert_eq!(std::fs::read(&lp).unwrap(), l_bytes, "{name} original bytes untouched");
    }

    // -----------------------------------------------------------------------
    // 3. Tables & spreadsheets (6 formats) -> Session::Table
    // -----------------------------------------------------------------------
    let dbf_l = left_dir.join("clientes.dbf");
    let dbf_r = right_dir.join("clientes.dbf");
    std::fs::write(&dbf_l, make_minimal_dbf(&[("1", "Alice"), ("2", "Bruno")])).unwrap();
    std::fs::write(&dbf_r, make_minimal_dbf(&[("1", "Alice"), ("2", "Carlos")])).unwrap();

    let table_pairs: Vec<(&str, PathBuf, PathBuf)> = vec![
        ("csv", repo.join("tests/fixtures/tables/left.csv"), repo.join("tests/fixtures/tables/right.csv")),
        ("tsv", repo.join("tests/fixtures/tables/left.tsv"), repo.join("tests/fixtures/tables/right.tsv")),
        ("xlsx", repo.join("tests/fixtures/tables/left.xlsx"), repo.join("tests/fixtures/tables/right.xlsx")),
        ("ods", repo.join("tests/fixtures/tables/left.ods"), repo.join("tests/fixtures/tables/right.ods")),
        ("html-table", repo.join("tests/fixtures/pe/multi-left.html"), repo.join("tests/fixtures/pe/multi-right.html")),
        ("dbf", dbf_l, dbf_r),
    ];
    for (label, lp, rp) in table_pairs {
        assert_eq!(SessionKind::for_paths(&lp, &rp), SessionKind::Table, "routing {label}");
        let mut app = drop_app();
        let mut dialogs = ui::dialogs::TestDialogs::default();
        app.handle_dropped_paths(&[lp, rp], None, &mut dialogs);
        assert!(dialogs.errors.is_empty(), "{label}: {:?}", dialogs.errors);
        let Session::Table(screen) = &app.sessions[0] else {
            panic!("expected Session::Table for {label}");
        };
        assert!(screen.error.is_none(), "{label} table error: {:?}", screen.error);
    }

    // -----------------------------------------------------------------------
    // 4. Raster images (7 real codecs via Windows WIC) -> Session::Picture
    // -----------------------------------------------------------------------
    const JPEG_L: &str = "ffd8ffe000104a46494600010100000100010000ffdb004300080606070605080707070909080a0c140d0c0b0b0c1912130f141d1a1f1e1d1a1c1c20242e2720222c231c1c2837292c30313434341f27393d38323c2e333432ffdb0043010909090c0b0c180d0d1832211c213232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232ffc00011080008000803012200021101031101ffc4001f0000010501010101010100000000000000000102030405060708090a0bffc400b5100002010303020403050504040000017d01020300041105122131410613516107227114328191a1082342b1c11552d1f02433627282090a161718191a25262728292a3435363738393a434445464748494a535455565758595a636465666768696a737475767778797a838485868788898a92939495969798999aa2a3a4a5a6a7a8a9aab2b3b4b5b6b7b8b9bac2c3c4c5c6c7c8c9cad2d3d4d5d6d7d8d9dae1e2e3e4e5e6e7e8e9eaf1f2f3f4f5f6f7f8f9faffc4001f0100030101010101010101010000000000000102030405060708090a0bffc400b51100020102040403040705040400010277000102031104052131061241510761711322328108144291a1b1c109233352f0156272d10a162434e125f11718191a262728292a35363738393a434445464748494a535455565758595a636465666768696a737475767778797a82838485868788898a92939495969798999aa2a3a4a5a6a7a8a9aab2b3b4b5b6b7b8b9bac2c3c4c5c6c7c8c9cad2d3d4d5d6d7d8d9dae2e3e4e5e6e7e8e9eaf2f3f4f5f6f7f8f9faffda000c03010002110311003f00e768a28af10fd40fffd9";
    const JPEG_R: &str = "ffd8ffe000104a46494600010100000100010000ffdb004300080606070605080707070909080a0c140d0c0b0b0c1912130f141d1a1f1e1d1a1c1c20242e2720222c231c1c2837292c30313434341f27393d38323c2e333432ffdb0043010909090c0b0c180d0d1832211c213232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232ffc00011080008000803012200021101031101ffc4001f0000010501010101010100000000000000000102030405060708090a0bffc400b5100002010303020403050504040000017d01020300041105122131410613516107227114328191a1082342b1c11552d1f02433627282090a161718191a25262728292a3435363738393a434445464748494a535455565758595a636465666768696a737475767778797a838485868788898a92939495969798999aa2a3a4a5a6a7a8a9aab2b3b4b5b6b7b8b9bac2c3c4c5c6c7c8c9cad2d3d4d5d6d7d8d9dae1e2e3e4e5e6e7e8e9eaf1f2f3f4f5f6f7f8f9faffc4001f0100030101010101010101010000000000000102030405060708090a0bffc400b51100020102040403040705040400010277000102031104052131061241510761711322328108144291a1b1c109233352f0156272d10a162434e125f11718191a262728292a35363738393a434445464748494a535455565758595a636465666768696a737475767778797a82838485868788898a92939495969798999aa2a3a4a5a6a7a8a9aab2b3b4b5b6b7b8b9bac2c3c4c5c6c7c8c9cad2d3d4d5d6d7d8d9dae2e3e4e5e6e7e8e9eaf2f3f4f5f6f7f8f9faffda000c03010002110311003f00f3ca28a2bf50388fffd9";
    const WEBP_L: &str = "524946463a00000057454250565038202e000000d001009d012a0800080001402625a00274ba01f80003b000feef76d7fe157c6e42d6fffbe807bd00f7a01fe08000";
    const WEBP_R: &str = "524946463800000057454250565038202c0000009001009d012a0800080001402625a00274ba00039800fef7592ffefa01ff7a01ff7a01fe2ffaf29f12b40000";
    const GIF_L: &str = "47494638376108000800810000dc28280000000000000000002c000000000800080000080f0001081c48b0a0c18308132a4c1810003b";
    const GIF_R: &str = "474946383761080008008100002828dc0000000000000000002c000000000800080000080f0001081c48b0a0c18308132a4c1810003b";
    const BMP_L: &str = "424df600000000000000360000002800000008000000080000000100180000000000c0000000c40e0000c40e000000000000000000002828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc";
    const BMP_R: &str = "424df600000000000000360000002800000008000000080000000100180000000000c0000000c40e0000c40e00000000000000000000dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828";
    const TIFF_L: &str = "49492a00080000000a0000010400010000000800000001010400010000000800000002010300030000008600000003010300010000000100000006010300010000000200000011010400010000008c0000001501030001000000030000001601040001000000080000001701040001000000c00000001c010300010000000100000000000000080008000800dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828";
    const TIFF_R: &str = "49492a00080000000a0000010400010000000800000001010400010000000800000002010300030000008600000003010300010000000100000006010300010000000200000011010400010000008c0000001501030001000000030000001601040001000000080000001701040001000000c00000001c0103000100000001000000000000000800080008002828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc2828dc";
    let img_cases: Vec<(&str, Vec<u8>, Vec<u8>)> = vec![
        ("pic.png", make_test_png([220, 40, 40, 255]), make_test_png([40, 40, 220, 255])),
        ("pic.jpg", decode_hex_bytes(JPEG_L), decode_hex_bytes(JPEG_R)),
        ("pic.bmp", decode_hex_bytes(BMP_L), decode_hex_bytes(BMP_R)),
        ("pic.gif", decode_hex_bytes(GIF_L), decode_hex_bytes(GIF_R)),
        ("pic.ico", make_test_ico([220, 40, 40, 255]), make_test_ico([40, 40, 220, 255])),
        ("pic.tif", decode_hex_bytes(TIFF_L), decode_hex_bytes(TIFF_R)),
        ("pic.webp", decode_hex_bytes(WEBP_L), decode_hex_bytes(WEBP_R)),
    ];
    for (name, l_bytes, r_bytes) in img_cases {
        let lp = left_dir.join(name);
        let rp = right_dir.join(name);
        std::fs::write(&lp, &l_bytes).unwrap();
        std::fs::write(&rp, &r_bytes).unwrap();
        assert_eq!(SessionKind::for_paths(&lp, &rp), SessionKind::Picture, "routing {name}");
        crate::picture::PictureImage::load(&lp)
            .unwrap_or_else(|e| panic!("PictureImage::load({name}): {e}"));
        let mut app = drop_app();
        let mut dialogs = ui::dialogs::TestDialogs::default();
        app.handle_dropped_paths(&[lp, rp], None, &mut dialogs);
        assert!(dialogs.errors.is_empty(), "{name}: {:?}", dialogs.errors);
        let Session::Picture(screen) = &app.sessions[0] else {
            panic!("expected Session::Picture for {name}");
        };
        assert!(!screen.is_empty(), "{name} must load into PictureCompareScreen");
    }

    // -----------------------------------------------------------------------
    // 5. Windows PE Executables / DLLs (2 formats) -> Session::Version
    // -----------------------------------------------------------------------
    for ext in ["exe", "dll"] {
        let lp = left_dir.join(format!("app.{ext}"));
        let rp = right_dir.join(format!("app.{ext}"));
        std::fs::copy(repo.join("tests/fixtures/pe/pe32.bin"), &lp).unwrap();
        std::fs::copy(repo.join("tests/fixtures/pe/pe32-changed.bin"), &rp).unwrap();
        assert_eq!(SessionKind::for_paths(&lp, &rp), SessionKind::Version, "routing .{ext}");
        let mut app = drop_app();
        let mut dialogs = ui::dialogs::TestDialogs::default();
        app.handle_dropped_paths(&[lp, rp], None, &mut dialogs);
        assert!(dialogs.errors.is_empty(), ".{ext}: {:?}", dialogs.errors);
        assert!(matches!(app.sessions[0], Session::Version(_)), "expected Session::Version for .{ext}");
    }

    // -----------------------------------------------------------------------
    // 6. Media files (3 formats: MP3, WAV, MP4) -> Session::Properties(Media)
    // -----------------------------------------------------------------------
    let wav_l = left_dir.join("tone.wav");
    let wav_r = right_dir.join("tone.wav");
    std::fs::write(&wav_l, make_minimal_wav(&[0, 1000, -1000, 0])).unwrap();
    std::fs::write(&wav_r, make_minimal_wav(&[0, 2000, -2000, 0])).unwrap();

    let mp4_l = left_dir.join("clip.mp4");
    let mp4_r = right_dir.join("clip.mp4");
    std::fs::write(&mp4_l, make_minimal_mp4(b"mp42", "Faixa A")).unwrap();
    std::fs::write(&mp4_r, make_minimal_mp4(b"mp42", "Faixa B")).unwrap();

    let mut media_pairs: Vec<(&str, PathBuf, PathBuf)> = vec![
        ("wav", wav_l, wav_r),
        ("mp4", mp4_l, mp4_r),
    ];
    let mp3_a = repo.join("vendor/media/tests/fixtures/A.mp3");
    let mp3_b = repo.join("vendor/media/tests/fixtures/B.mp3");
    if mp3_a.exists() && mp3_b.exists() {
        media_pairs.push(("mp3", mp3_a, mp3_b));
    }
    for (label, lp, rp) in media_pairs {
        assert_eq!(SessionKind::for_paths(&lp, &rp), SessionKind::Media, "routing {label}");
        let mut app = drop_app();
        let mut dialogs = ui::dialogs::TestDialogs::default();
        app.handle_dropped_paths(&[lp, rp], None, &mut dialogs);
        assert!(dialogs.errors.is_empty(), "{label}: {:?}", dialogs.errors);
        assert!(
            matches!(&app.sessions[0], Session::Properties(s) if s.kind() == SessionKind::Media),
            "expected Session::Properties(Media) for {label}"
        );
    }

    // -----------------------------------------------------------------------
    // 7. Windows Registry (.reg) -> Session::Properties(Registry)
    // -----------------------------------------------------------------------
    let reg_l = left_dir.join("settings.reg");
    let reg_r = right_dir.join("settings.reg");
    std::fs::write(
        &reg_l,
        "Windows Registry Editor Version 5.00\r\n\r\n[HKEY_CURRENT_USER\\Software\\AnyCompareTest]\r\n\"Mode\"=\"Alpha\"\r\n",
    )
    .unwrap();
    std::fs::write(
        &reg_r,
        "Windows Registry Editor Version 5.00\r\n\r\n[HKEY_CURRENT_USER\\Software\\AnyCompareTest]\r\n\"Mode\"=\"Beta\"\r\n",
    )
    .unwrap();
    assert_eq!(SessionKind::for_paths(&reg_l, &reg_r), SessionKind::Registry);
    {
        let mut app = drop_app();
        let mut dialogs = ui::dialogs::TestDialogs::default();
        app.handle_dropped_paths(&[reg_l, reg_r], None, &mut dialogs);
        assert!(dialogs.errors.is_empty(), ".reg: {:?}", dialogs.errors);
        assert!(
            matches!(&app.sessions[0], Session::Properties(s) if s.kind() == SessionKind::Registry)
        );
    }

    // -----------------------------------------------------------------------
    // 8. Folders, Archives (.zip, .tar, .tar.gz) & Snapshots (.acsnap.json) -> Session::Folder
    // -----------------------------------------------------------------------
    let zip_l = base.join("pkg_left.zip");
    let zip_r = base.join("pkg_right.zip");
    std::fs::write(
        &zip_l,
        make_zip_bytes(&[
            ("same.txt", b"identical"),
            ("sub/changed.txt", b"version 1"),
            ("only_left.txt", b"orphan L"),
        ]),
    )
    .unwrap();
    std::fs::write(
        &zip_r,
        make_zip_bytes(&[
            ("same.txt", b"identical"),
            ("sub/changed.txt", b"version 2"),
            ("only_right.txt", b"orphan R"),
        ]),
    )
    .unwrap();

    let tar_l = base.join("pkg_left.tar");
    let tar_r = base.join("pkg_right.tar");
    let tar_l_bytes = make_tar_bytes(&[("same.txt", b"ok"), ("diff.txt", b"AAA")]);
    let tar_r_bytes = make_tar_bytes(&[("same.txt", b"ok"), ("diff.txt", b"BBB")]);
    std::fs::write(&tar_l, &tar_l_bytes).unwrap();
    std::fs::write(&tar_r, &tar_r_bytes).unwrap();

    let tgz_l = base.join("pkg_left.tar.gz");
    let tgz_r = base.join("pkg_right.tar.gz");
    for (dst, raw) in [(&tgz_l, &tar_l_bytes), (&tgz_r, &tar_r_bytes)] {
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(raw).unwrap();
        std::fs::write(dst, gz.finish().unwrap()).unwrap();
    }

    let snap_dir_l = base.join("snap_left");
    let snap_dir_r = base.join("snap_right");
    std::fs::create_dir_all(&snap_dir_l).unwrap();
    std::fs::create_dir_all(&snap_dir_r).unwrap();
    std::fs::write(snap_dir_l.join("a.txt"), b"equal").unwrap();
    std::fs::write(snap_dir_l.join("b.txt"), b"old-data").unwrap();
    std::fs::write(snap_dir_r.join("a.txt"), b"equal").unwrap();
    std::fs::write(snap_dir_r.join("b.txt"), b"new-data").unwrap();
    let snap_file = base.join("left.acsnap.json");
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let snap_count = crate::folder_source::save_snapshot(&snap_dir_l, &snap_file, &cancel).unwrap();
    assert_eq!(snap_count, 2);

    for (label, lp, rp) in [
        ("dirs", &snap_dir_l, &snap_dir_r),
        ("zip-vs-zip", &zip_l, &zip_r),
        ("tar-vs-tar", &tar_l, &tar_r),
        ("tgz-vs-tgz", &tgz_l, &tgz_r),
        ("snapshot-vs-dir", &snap_file, &snap_dir_r),
    ] {
        assert_eq!(SessionKind::for_paths(lp, rp), SessionKind::Folder, "routing {label}");
        let mut app = drop_app();
        let mut dialogs = ui::dialogs::TestDialogs::default();
        app.handle_dropped_paths(&[lp.clone(), rp.clone()], None, &mut dialogs);
        assert!(dialogs.errors.is_empty(), "{label}: {:?}", dialogs.errors);
        let Session::Folder(screen) = &mut app.sessions[0] else {
            panic!("expected Session::Folder for {label}");
        };
        let results = screen.comparison_results();
        assert!(
            results.iter().any(|(_, st)| *st == "same"),
            "{label}: expected at least one 'same' entry, got {results:?}"
        );
        assert!(
            results.iter().any(|(_, st)| *st == "different"),
            "{label}: expected at least one 'different' entry, got {results:?}"
        );
    }

    // Verify double-click materialization inside a ZIP archive yields a readable temp file
    let materialized =
        crate::folder_source::materialize_file(&zip_l, std::path::Path::new("sub/changed.txt"))
            .unwrap();
    assert_eq!(std::fs::read(&materialized).unwrap(), b"version 1");
    // Verify snapshot materialization fails gracefully with localized read-only error
    assert!(
        crate::folder_source::materialize_file(&snap_file, std::path::Path::new("a.txt")).is_err()
    );

    // -----------------------------------------------------------------------
    // 9. Raw binary fallback (.bin) -> Session::Hex
    // -----------------------------------------------------------------------
    let bin_l = left_dir.join("firmware.bin");
    let bin_r = right_dir.join("firmware.bin");
    std::fs::write(&bin_l, [0x00u8, 0xFF, 0x10, 0x20, 0xAA, 0x00, 0x01, 0x02]).unwrap();
    std::fs::write(&bin_r, [0x00u8, 0xFF, 0x10, 0x99, 0xAA, 0x00, 0x01, 0x02]).unwrap();
    {
        let mut app = drop_app();
        let mut dialogs = ui::dialogs::TestDialogs::default();
        app.handle_dropped_paths(&[bin_l, bin_r], None, &mut dialogs);
        assert!(dialogs.errors.is_empty(), ".bin: {:?}", dialogs.errors);
        assert!(
            matches!(app.sessions[0], Session::Hex(_)),
            "expected Session::Hex fallback for raw binary .bin files"
        );
    }

    let _ = std::fs::remove_dir_all(&base);
}
