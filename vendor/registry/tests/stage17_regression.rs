//! Regressão de tipos, exportação e limites — testes Rust a executar com Cargo.
use bc5_registry::{compare, export_reg, export_subtree, parse_reg, Difference, RegistryView};
const HEADER: &str = "Windows Registry Editor Version 5.00\n";
fn reg(lines: &str) -> bc5_registry::Registry {
    parse_reg(format!("{HEADER}{lines}").as_bytes()).unwrap()
}
#[test]
fn raw_types_are_not_collapsed_into_displayed_text() {
    let a = reg("[HKEY_CURRENT_USER\\X]\n\"A\"=hex(2):31,00,00,00\n");
    let b = reg("[HKEY_CURRENT_USER\\X]\n\"A\"=hex(1):31,00,00,00\n");
    assert_eq!(compare(&a, &b).changed_count(), 1);
}
#[test]
fn export_roundtrip_keeps_binary_and_unknown_id() {
    let original =
        reg("[HKEY_CURRENT_USER\\X]\n\"custom\"=hex(99):01,00,ff,80\n\"number\"=dword:fedcba98\n");
    let restored = parse_reg(&export_reg(&original).unwrap()).unwrap();
    assert_eq!(compare(&original, &restored).changed_count(), 0);
    assert!(compare(&original, &restored)
        .rows
        .iter()
        .all(|r| r.status == Difference::Same));
}
#[test]
fn export_hex_wrap_roundtrip() {
    let mut input = String::from(HEADER);
    input.push_str("[HKEY_CURRENT_USER\\X]\n\"big\"=hex:");
    for i in 0..160u16 {
        if i > 0 {
            input.push(',');
        }
        input.push_str(&format!("{:02x}", i as u8));
    }
    input.push('\n');
    let original = parse_reg(input.as_bytes()).unwrap();
    let encoded = export_reg(&original).unwrap();
    let restored = parse_reg(&encoded).unwrap();
    assert_eq!(original, restored);
}
#[test]
fn subtree_does_not_include_neighbor_prefix() {
    let original = reg("[HKEY_CURRENT_USER\\X]\n@=\"1\"\n[HKEY_CURRENT_USER\\X1]\n@=\"2\"\n");
    let output = export_subtree(&original, "HKEY_CURRENT_USER\\X").unwrap();
    let sub = parse_reg(&output).unwrap();
    assert!(!sub.keys.contains_key("hkey_current_user\\x1"));
}
#[test]
fn invalid_key_paths_rejected() {
    for path in [
        "HKEY_CURRENT_USER\\",
        "HKEY_CURRENT_USER\\\\X",
        "\\HKEY_CURRENT_USER",
    ] {
        assert!(
            parse_reg(format!("{HEADER}[{path}]\n").as_bytes()).is_err(),
            "{path}"
        );
    }
}
#[test]
fn same_duplicate_value_different_case_keeps_one() {
    let r = reg("[HKEY_CURRENT_USER\\X]\n\"Name\"=dword:00000001\n\"name\"=dword:00000001\n");
    assert_eq!(r.keys["hkey_current_user\\x"].values.len(), 1);
}
#[cfg(not(windows))]
#[test]
fn live_registry_requires_windows() {
    assert!(
        bc5_registry::read_live_with_options(None, "HKEY_CURRENT_USER", RegistryView::Native)
            .is_err()
    );
}
