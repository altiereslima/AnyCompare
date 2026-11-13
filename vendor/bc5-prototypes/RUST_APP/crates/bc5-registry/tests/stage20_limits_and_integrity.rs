//! Etapa 20: testes de limites e integridade das operações offline.
//! Não equivalem a resultados A/B do BC5 original.
use bc5_registry::{export_reg, export_subtree, parse_reg, RegValue, Registry, SnapshotEditor, MAX_REG_BYTES};

const HEADER: &str = "Windows Registry Editor Version 5.00\n";
fn fixture() -> Registry {
    parse_reg(format!("{HEADER}[HKEY_CURRENT_USER\\A]\n\"Texto\"=\"abc\"\n[HKEY_CURRENT_USER\\A\\B]\n\"N\"=dword:12345678\n").as_bytes()).unwrap()
}

#[test]
fn copy_from_forged_value_identity_rejects_without_mutation() {
    let mut source=fixture();
    source.keys.get_mut("hkey_current_user\\a").unwrap().values.insert(
        "not-the-name".to_owned(),
        RegValue{name:"Texto".to_owned(),type_id:3,bytes:vec![1,2,3]});
    let mut target=SnapshotEditor::new(fixture()).unwrap();
    let initial=target.registry.clone();
    assert!(target.copy_value_from(&source,"HKEY_CURRENT_USER\\A","Texto","HKEY_CURRENT_USER\\A\\B").is_err());
    assert_eq!(target.registry,initial);
    assert_eq!(target.undo_count(),0);
}

#[test]
fn copy_from_forged_key_identity_rejects_without_mutation() {
    let mut source=fixture();
    source.keys.get_mut("hkey_current_user\\a").unwrap().path="HKEY_CURRENT_USER\\Outra".to_string();
    let mut target=SnapshotEditor::new(fixture()).unwrap();
    assert!(target.copy_value_from(&source,"HKEY_CURRENT_USER\\A","Texto","HKEY_CURRENT_USER\\A\\B").is_err());
    assert_eq!(target.undo_count(),0);
}

#[test]
fn oversized_value_source_rejected_before_copy() {
    let mut source=fixture();
    source.keys.get_mut("hkey_current_user\\a").unwrap().values.insert(
        "huge".into(),RegValue{name:"Huge".into(),type_id:3,bytes:vec![0;8*1024*1024+1]});
    let mut target=SnapshotEditor::new(fixture()).unwrap();
    assert!(target.copy_key_from(&source,"HKEY_CURRENT_USER\\A\\B","HKEY_CURRENT_USER\\A","C").is_err());
    assert_eq!(target.undo_count(),0);
    assert!(!target.registry.keys.contains_key("hkey_current_user\\a\\c"));
}

#[test]
fn export_rejects_unserializable_large_raw_value() {
    let mut registry=fixture();
    registry.keys.get_mut("hkey_current_user\\a").unwrap().values.insert(
        "oversized".into(),RegValue{name:"Oversized".into(),type_id:3,
            bytes:vec![0xAA;MAX_REG_BYTES/4+1]});
    assert!(export_reg(&registry).is_err());
}

#[test]
fn subtree_export_does_not_include_large_unselected_branch() {
    let mut registry=fixture();
    registry.keys.get_mut("hkey_current_user\\a").unwrap().values.insert(
        "oversized".into(),RegValue{name:"Oversized".into(),type_id:3,
            bytes:vec![0xAA;MAX_REG_BYTES/4+1]});
    let sub=export_subtree(&registry,"HKEY_CURRENT_USER\\A\\B").unwrap();
    let recovered=parse_reg(&sub).unwrap();
    assert!(recovered.keys.contains_key("hkey_current_user\\a\\b"));
    assert!(recovered.keys["hkey_current_user\\a"].values.is_empty());
}

#[test]
fn snapshot_copy_fails_closed_on_duplicate_identity() {
    let mut src=fixture();
    src.keys.get_mut("hkey_current_user\\a").unwrap().values.insert(
        "tExTo".into(),RegValue{name:"Texto".into(),type_id:3,bytes:vec![]});
    let mut dst=SnapshotEditor::new(fixture()).unwrap();
    let previous=dst.registry.clone();
    assert!(dst.copy_key_from(&src,"HKEY_CURRENT_USER\\A\\B","HKEY_CURRENT_USER\\A","C").is_err());
    assert_eq!(dst.registry,previous);
}
