//! Etapa 21: destaque intravalor e prévias, NÃO equivalência 1:1.
use bc5_registry::{character_difference, compare, parse_reg, Difference, ItemKind, RegValue};
fn text(s: &str) -> RegValue {
    let mut bytes = Vec::new();
    for u in s.encode_utf16().chain(std::iter::once(0)) {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    RegValue {
        name: "Texto".into(),
        type_id: 1,
        bytes,
    }
}
#[test]
fn change_at_end_of_large_binary_remains_a_difference_despite_bounded_preview() {
    let a = RegValue {
        name: "Bin".into(),
        type_id: 3,
        bytes: vec![0xAA; 8192],
    };
    let mut b = a.clone();
    *b.bytes.last_mut().unwrap() = 0xBB;
    assert_eq!(a.displayed_preview(64), b.displayed_preview(64));
    assert!(character_difference(&a, &b).is_none());
    // O destaque é somente apresentação; igualdade é definida pelos bytes completos.
    let reg =
        "Windows Registry Editor Version 5.00\n[HKEY_CURRENT_USER\\Test]\n\"Bin\"=hex:aa,bb\n";
    let r = parse_reg(reg.as_bytes()).unwrap();
    let mut r2 = r.clone();
    r2.keys
        .get_mut("hkey_current_user\\test")
        .unwrap()
        .values
        .get_mut("bin")
        .unwrap()
        .bytes
        .push(0);
    let diff = compare(&r, &r2);
    assert!(diff
        .rows
        .iter()
        .any(|x| x.item == ItemKind::Value && x.status == Difference::Changed));
}
#[test]
fn unicode_prefix_suffix_preserved_without_changing_types_or_bytes() {
    let left = text("Início 🍎 conclusão");
    let right = text("Início 🍇 conclusão");
    let previous = left.bytes.clone();
    let d = character_difference(&left, &right).unwrap();
    assert_eq!(
        (
            d.common_prefix,
            d.left_changed,
            d.right_changed,
            d.common_suffix
        ),
        (
            "Início ".into(),
            "🍎".into(),
            "🍇".into(),
            " conclusão".into()
        )
    );
    assert_eq!(left.bytes, previous);
}
#[test]
fn display_preview_is_small_and_explicit_about_truncation() {
    let huge = RegValue {
        name: "bytes".into(),
        type_id: 3,
        bytes: vec![0x81; 8 * 1024 * 1024],
    };
    let view = huge.displayed_preview(32);
    assert!(view.len() < 200);
    assert!(view.contains("8388608 bytes"));
    let short = text("café").displayed_preview(256);
    assert_eq!(short, "café");
}
#[test]
fn binary_type_difference_alone_has_no_character_alignment() {
    let a = RegValue {
        name: "Value".into(),
        type_id: 3,
        bytes: vec![1, 2, 3],
    };
    let mut b = a.clone();
    b.type_id = 0;
    assert!(character_difference(&a, &b).is_none());
}
