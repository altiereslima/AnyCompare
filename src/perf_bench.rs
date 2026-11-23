//! Reproducible release-mode performance benchmark suite (`cargo test --release --offline --locked perf_bench -- --ignored --nocapture`).
use crate::{
    binary_diff::BinaryDiff,
    diff::{DiffEngine, DiffRules},
    history::SessionKind,
    picture::{compare_pictures, CompareOptions, ImageTransform, PictureImage},
    ui::{
        hex_compare::HexCompareScreen,
        text_compare::TextCompareScreen,
        version_compare::VersionCompareScreen,
    },
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

fn make_temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("anycompare_perf_{name}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn synthetic_wav(payload_bytes: usize, seed: u8) -> Vec<u8> {
    let mut buf = Vec::with_capacity(44 + payload_bytes);
    let file_size = (36 + payload_bytes) as u32;
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&file_size.to_le_bytes());
    buf.extend_from_slice(b"WAVEfmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&2u16.to_le_bytes()); // stereo
    buf.extend_from_slice(&44100u32.to_le_bytes());
    buf.extend_from_slice(&176400u32.to_le_bytes());
    buf.extend_from_slice(&4u16.to_le_bytes());
    buf.extend_from_slice(&16u16.to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&(payload_bytes as u32).to_le_bytes());
    let mut state = 0x1234_5678u32 ^ (seed as u32);
    for _ in 0..payload_bytes {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        buf.push((state >> 24) as u8);
    }
    buf
}

fn synthetic_bin(size: usize, seed: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(size);
    let mut x = seed ^ 0x9E37_79B9;
    for _ in 0..size {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        out.push((x & 0xFF) as u8);
    }
    out
}

#[test]
#[ignore]
fn perf_bench_all() {
    let tmp = make_temp_dir("suite");

    // 1. Real large text compare (ingles.txt 1.5MB vs portugues.txt 2.5MB)
    let real_l = Path::new(r"c:\Projetos\BC5\teste_desempenho\ingles.txt");
    let real_r = Path::new(r"c:\Projetos\BC5\teste_desempenho\portugues.txt");
    if real_l.exists() && real_r.exists() {
        let t0 = Instant::now();
        let mut screen = TextCompareScreen::new();
        screen.load_files(real_l.to_str().unwrap(), real_r.to_str().unwrap());
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        println!(
            "BENCH text_real_ingles_vs_portugues_ms={ms:.2} (lines={})",
            screen.diff_result.lines.len()
        );
    }

    // 2. Large CRLF text load + diff (2 x 5 MB)
    let mut big_l = String::with_capacity(5_200_000);
    for i in 0..80_000usize {
        big_l.push_str(&format!("line_{i:06}: The quick brown fox jumps over the lazy dog 1234567890\r\n"));
    }
    let mut big_r = big_l.clone();
    big_r.replace_range(100..110, "MODIFIED!!");
    let mid = big_r.len() / 2;
    big_r.replace_range(mid..mid + 10, "MIDDLE_CHG");
    let tl_path = tmp.join("big_l.txt");
    let tr_path = tmp.join("big_r.txt");
    fs::write(&tl_path, &big_l).unwrap();
    fs::write(&tr_path, &big_r).unwrap();

    let t0 = Instant::now();
    let mut txt_screen = TextCompareScreen::new();
    txt_screen.load_files(tl_path.to_str().unwrap(), tr_path.to_str().unwrap());
    let text_load_ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!(
        "BENCH text_5mb_load_and_diff_ms={text_load_ms:.2} (lines={})",
        txt_screen.diff_result.lines.len()
    );

    // 3. Text incremental editing (30 single-char edits on 5 MB buffer)
    let rules = DiffRules::default();
    let mut cur_left = txt_screen.left.text.clone();
    let right_ref = txt_screen.right.text.clone();
    let mut diff_state = txt_screen.diff_result.clone();
    let edit_pos = cur_left.len() / 2;
    let t0 = Instant::now();
    for k in 0..30usize {
        let prev = cur_left.clone();
        cur_left.insert(edit_pos + k, 'x');
        diff_state = DiffEngine::update_edit_rules(&prev, &cur_left, &right_ref, true, &diff_state, rules);
    }
    let text_edit_ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!(
        "BENCH text_5mb_30_edits_ms={text_edit_ms:.2} ({:.2} ms/edit, diffs={})",
        text_edit_ms / 30.0,
        diff_state.diff_count
    );

    // 3b. Text incremental editing IN-PLACE (30 single-char edits on 5 MB buffer)
    let mut cur_left_ip = txt_screen.left.text.clone();
    let mut diff_state_ip = txt_screen.diff_result.clone();
    let t0 = Instant::now();
    for k in 0..30usize {
        let prev = cur_left_ip.clone();
        cur_left_ip.insert(edit_pos + k, 'x');
        DiffEngine::update_edit_rules_in_place(
            &prev,
            &cur_left_ip,
            &right_ref,
            true,
            &mut diff_state_ip,
            rules,
        );
    }
    let text_edit_ip_ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!(
        "BENCH text_5mb_30_edits_in_place_ms={text_edit_ip_ms:.2} ({:.2} ms/edit, diffs={})",
        text_edit_ip_ms / 30.0,
        diff_state_ip.diff_count
    );
    assert_eq!(diff_state_ip, diff_state);

    // 4. Hex Load + Diff: 2 x 4 MB binary with 2 scattered byte edits (offset 1KB and 3.9MB)
    let hex_a = synthetic_bin(4 * 1024 * 1024, 1);
    let mut hex_b = hex_a.clone();
    hex_b[1024] ^= 0xFF;
    hex_b[4 * 1024 * 1024 - 2048] ^= 0xAA;
    let hl_path = tmp.join("hex_l.bin");
    let hr_path = tmp.join("hex_r.bin");
    fs::write(&hl_path, &hex_a).unwrap();
    fs::write(&hr_path, &hex_b).unwrap();

    let t0 = Instant::now();
    let mut hex_screen = HexCompareScreen::new();
    hex_screen.load_files(&hl_path, &hr_path);
    let hex_scattered_ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!("BENCH hex_4mb_scattered_load_diff_ms={hex_scattered_ms:.2}");

    // 5. Hex Diff: 2 x 4 MB completely different binaries
    let hex_c = synthetic_bin(4 * 1024 * 1024, 999);
    let t0 = Instant::now();
    let d_unrel = BinaryDiff::compare(&hex_a, &hex_c);
    let hex_unrelated_ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!(
        "BENCH hex_4mb_unrelated_diff_ms={hex_unrelated_ms:.2} (diffs={}, limited={})",
        d_unrel.difference_count(),
        d_unrel.limited
    );

    // 6. Hex Editing: 3 single-byte edits on 4 MB file (each currently stalls up to 4000 ms in Myers!)
    let t0 = Instant::now();
    for k in 0..3usize {
        hex_screen.bench_replace_byte(true, 2000 + k * 17, (k as u8).wrapping_mul(37));
    }
    let hex_edit_ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!(
        "BENCH hex_4mb_3_edits_ms={hex_edit_ms:.2} ({:.2} ms/edit)",
        hex_edit_ms / 3.0
    );

    // 6b. Hex next_diff scan from start to end of 4 MB file (20 jumps)
    let t0 = Instant::now();
    for _ in 0..20 {
        hex_screen.bench_next_diff_cycle();
    }
    let hex_nav_ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!("BENCH hex_4mb_20_next_diff_ms={hex_nav_ms:.2}");

    // 7. Media Compare Load: 2 x 16 MB WAV files
    let wav_l = tmp.join("media_l.wav");
    let wav_r = tmp.join("media_r.wav");
    fs::write(&wav_l, synthetic_wav(16 * 1024 * 1024, 1)).unwrap();
    fs::write(&wav_r, synthetic_wav(16 * 1024 * 1024, 2)).unwrap();
    let t0 = Instant::now();
    let mut media_screen = VersionCompareScreen::for_kind(SessionKind::Media);
    media_screen.load_files(&wav_l, &wav_r).unwrap();
    let media_ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!("BENCH media_2x16mb_wav_load_ms={media_ms:.2}");

    // 8. Picture Compare Diff: 2 x 4K (3840x2160) RGBA images
    let w = 3840u32;
    let h = 2160u32;
    let rgba_l = vec![120u8; (w * h * 4) as usize];
    let mut rgba_r = rgba_l.clone();
    for i in (0..rgba_r.len()).step_by(4096) {
        rgba_r[i] = 240;
    }
    let img_l = PictureImage::from_rgba(w, h, rgba_l, false);
    let img_r = PictureImage::from_rgba(w, h, rgba_r, false);
    let t0 = Instant::now();
    let pic_cmp = compare_pictures(
        Some(&img_l),
        Some(&img_r),
        ImageTransform::default(),
        ImageTransform::default(),
        CompareOptions::default(),
    );
    let pic_ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!(
        "BENCH picture_4k_diff_ms={pic_ms:.2} (same={}, diff={})",
        pic_cmp.stats.same_pixels, pic_cmp.stats.different_pixels
    );

    // 9. Folder Scan (real target/release/build vs deps) + Binary Content Compare (160 x 128KB pairs = 20MB/side)
    let fld_l = Path::new(r"c:\Projetos\BC5\anycompare\target\release\build");
    let fld_r = Path::new(r"c:\Projetos\BC5\anycompare\target\release\deps");
    if fld_l.exists() && fld_r.exists() {
        let t0 = Instant::now();
        let count = crate::ui::folder_compare::bench_scan_folders(fld_l, fld_r, false);
        let scan_ms = t0.elapsed().as_secs_f64() * 1000.0;
        println!("BENCH folder_scan_parallel_ms={scan_ms:.2} (entries={count})");
    }

    let fl_dir = tmp.join("fld_l");
    let fr_dir = tmp.join("fld_r");
    fs::create_dir_all(&fl_dir).unwrap();
    fs::create_dir_all(&fr_dir).unwrap();
    let payload = synthetic_bin(128 * 1024, 42);
    for i in 0..160usize {
        let rel = PathBuf::from(format!("f_{i:03}.bin"));
        fs::write(fl_dir.join(&rel), &payload).unwrap();
        fs::write(fr_dir.join(&rel), &payload).unwrap();
    }
    let t0 = Instant::now();
    let compared = crate::ui::folder_compare::bench_scan_folders(&fl_dir, &fr_dir, true);
    let content_par_ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!("BENCH folder_content_160x128kb_parallel_ms={content_par_ms:.2} (entries={compared})");

    let _ = fs::remove_dir_all(&tmp);
}
