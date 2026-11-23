use bc5_registry::{compare, parse_reg, Difference, ItemKind};
const FIXTURES: &str = "../../../TESTES_REGISTRY/";
fn get(name: &str) -> Vec<u8> {
    // O caminho está ancorado no Cargo.toml do crate, não na pasta atual de execução.
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURES)
        .join(name);
    std::fs::read(p).unwrap()
}
#[test]
fn baseline_identical_across_case_and_hex4() {
    let l = parse_reg(&get("01_iguais_esquerda.reg")).unwrap();
    let r = parse_reg(&get("01_iguais_direita.reg")).unwrap();
    assert!(compare(&l, &r)
        .rows
        .iter()
        .all(|v| v.status == Difference::Same));
}
#[test]
fn changed_default_and_left_right_orphans() {
    let l = parse_reg(&get("02_diferencas_esquerda.reg")).unwrap();
    let r = parse_reg(&get("02_diferencas_direita.reg")).unwrap();
    let c = compare(&l, &r);
    assert_eq!(c.changed_count(), 2);
    assert_eq!(
        c.rows
            .iter()
            .filter(|v| v.item == ItemKind::Value && v.status == Difference::LeftOnly)
            .count(),
        1
    );
    assert_eq!(
        c.rows
            .iter()
            .filter(|v| v.item == ItemKind::Value && v.status == Difference::RightOnly)
            .count(),
        1
    );
}
#[test]
fn qword_multi_hex_continuation() {
    let l = parse_reg(&get("03_tipos_esquerda.reg")).unwrap();
    let r = parse_reg(&get("03_tipos_direita.reg")).unwrap();
    assert!(compare(&l, &r)
        .rows
        .iter()
        .all(|v| v.status == Difference::Same));
    let values = &l.keys["hkey_local_machine\\software\\bc5-teste"].values;
    assert_eq!(values["lista"].displayed(), "A | B");
    assert_eq!(values["qword"].type_id, 11);
    assert_eq!(values["binario"].bytes.len(), 7);
}
#[test]
fn missing_child_keys_propagate_to_parent() {
    let l = parse_reg(&get("04_chaves_esquerda.reg")).unwrap();
    let r = parse_reg(&get("04_chaves_direita.reg")).unwrap();
    let c = compare(&l, &r);
    assert!(c.rows.iter().any(|x| x.item == ItemKind::Key
        && x.name == "BC5-Teste"
        && x.status == Difference::Changed));
}
#[test]
fn do_not_confuse_scripts_and_snapshots() {
    assert!(parse_reg(&get("05_delecao_invalida.reg")).is_err());
    assert!(parse_reg(&get("06_duplicado_conflitante.reg")).is_err());
}
#[test]
fn compat_ascii_regedit4_and_utf8_bom() {
    assert!(parse_reg(&get("07_ascii_regedit4.reg")).is_ok());
    assert!(parse_reg(&get("08_utf8_bom.reg")).is_ok());
    assert!(parse_reg(&get("09_utf16_curto.reg")).is_ok());
}
