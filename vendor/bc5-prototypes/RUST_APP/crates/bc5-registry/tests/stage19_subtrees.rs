//! Etapa 19: contrato do editor offline. Testes apenas do Rust, ainda sem A/B BC5.
use bc5_registry::{compare, export_reg, parse_reg, Difference, RegKey, RegValue, Registry, SnapshotEditor};
use std::collections::BTreeMap;

fn from(text: &str) -> Registry {parse_reg(text.as_bytes()).expect("fixture .reg válido")}
const H: &str = "Windows Registry Editor Version 5.00\n";

fn source() -> Registry {
    from(&format!("{H}[HKEY_CURRENT_USER\\Origem]\n\"Padrão\"=hex(3):de,ad,be,ef\n\"N\"=dword:00000002\n[HKEY_CURRENT_USER\\Origem\\Filha]\n@=hex(7):41,00,00,00,00,00\n[HKEY_CURRENT_USER\\Origem\\Filha\\Neta]\n\"Tipo\"=hex(42):fe,ed\n"))
}
fn target() -> Registry {from(&format!("{H}[HKEY_CURRENT_USER\\Destino]\n\"Não alterar\"=dword:00000003\n"))}

#[test]
fn full_subtree_copy_preserves_names_types_and_bytes() {
    let src=source(); let target=target();
    let mut edit=SnapshotEditor::new(target.clone()).unwrap();
    edit.copy_key_from(&src,"hkey_current_user\\origem","HKEY_CURRENT_USER\\Destino","Cópia").unwrap();
    let key=&edit.registry.keys["hkey_current_user\\destino\\cópia"];
    assert_eq!(key.values["padrão"].type_id,3);
    assert_eq!(key.values["padrão"].bytes,[0xde,0xad,0xbe,0xef]);
    assert_eq!(edit.registry.keys["hkey_current_user\\destino\\cópia\\filha"].values[""].type_id,7);
    assert_eq!(edit.registry.keys["hkey_current_user\\destino\\cópia\\filha\\neta"].values["tipo"].bytes,[0xfe,0xed]);
    assert_eq!(edit.registry.keys["hkey_current_user\\destino"].values["não alterar"].bytes,[3,0,0,0]);
    assert!(compare(&target,&edit.registry).rows.iter().any(|r|r.status==Difference::RightOnly));
    let exported=export_reg(&edit.registry).unwrap();
    let recovered=parse_reg(&exported).unwrap();
    assert!(compare(&edit.registry,&recovered).rows.iter().all(|r|r.status==Difference::Same));
}

#[test]
fn copy_then_undo_redo_is_single_transaction() {
    let original=target(); let mut edit=SnapshotEditor::new(original.clone()).unwrap();
    edit.copy_key_from(&source(),"HKEY_CURRENT_USER\\Origem","hkey_current_user\\destino","Novo").unwrap();
    assert_eq!(edit.undo_count(),1);
    let copied=edit.registry.clone();
    edit.undo().unwrap(); assert_eq!(edit.registry,original);
    edit.redo().unwrap(); assert_eq!(edit.registry,copied);
}

#[test]
fn collision_rolls_back_without_consuming_undo() {
    let mut edit=SnapshotEditor::new(target()).unwrap();
    edit.new_key("HKEY_CURRENT_USER\\Destino\\Cópia").unwrap();
    let before=edit.registry.clone(); let undo=edit.undo_count();
    assert!(edit.copy_key_from(&source(),"HKEY_CURRENT_USER\\Origem","HKEY_CURRENT_USER\\Destino","cópia").is_err());
    assert_eq!(edit.registry,before);
    assert_eq!(edit.undo_count(),undo);
}

#[test]
fn missing_source_or_target_parent_rolls_back() {
    let src=source(); let mut edit=SnapshotEditor::new(target()).unwrap();
    let before=edit.registry.clone();
    assert!(edit.copy_key_from(&src,"HKEY_CURRENT_USER\\Ausente","HKEY_CURRENT_USER\\Destino","X").is_err());
    assert!(edit.copy_key_from(&src,"HKEY_CURRENT_USER\\Origem","HKEY_CURRENT_USER\\Ausente","X").is_err());
    assert!(edit.copy_key_from(&src,"HKEY_CURRENT_USER\\Origem","HKEY_CURRENT_USER\\Destino","In\\válido").is_err());
    assert!(edit.copy_key_from(&src,"HKEY_CURRENT_USER","HKEY_CURRENT_USER\\Destino","Raiz").is_err());
    assert_eq!(edit.registry,before);
}

#[test]
fn malformed_source_identity_rejected() {
    let mut src=source();
    src.keys.get_mut("hkey_current_user\\origem\\filha").unwrap().path="HKEY_CURRENT_USER\\Troca".into();
    let mut edit=SnapshotEditor::new(target()).unwrap();
    let before=edit.registry.clone();
    assert!(edit.copy_key_from(&src,"HKEY_CURRENT_USER\\Origem","HKEY_CURRENT_USER\\Destino","X").is_err());
    assert_eq!(edit.registry,before);
}

#[test]
fn missing_ancestors_cannot_be_edited_or_exported() {
    let mut registry=Registry::default();
    registry.keys.insert("hkey_current_user\\órfã".into(),RegKey{
        path:"HKEY_CURRENT_USER\\Órfã".into(),values:BTreeMap::new()});
    assert!(SnapshotEditor::new(registry.clone()).is_err());
    assert!(export_reg(&registry).is_err());
}

#[test]
fn forged_key_or_value_cannot_be_exported() {
    let mut registry=target();
    registry.keys.get_mut("hkey_current_user\\destino").unwrap().values.insert("outro".into(),
        RegValue{name:"Falso".into(),type_id:4,bytes:vec![2,0,0,0]});
    assert!(export_reg(&registry).is_err());
}

#[test]
fn copying_missing_key_to_same_path_is_supported_without_overwrite() {
    let src=source(); let mut dst=SnapshotEditor::new(target()).unwrap();
    // Mesmo caminho só funciona se o pai está presente no snapshot de destino.
    dst.copy_missing_key_from(&src,"HKEY_CURRENT_USER\\Origem").unwrap();
    assert!(dst.registry.keys.contains_key("hkey_current_user\\origem\\filha\\neta"));
    let after=dst.registry.clone();
    assert!(dst.copy_missing_key_from(&src,"HKEY_CURRENT_USER\\Origem").is_err());
    assert_eq!(dst.registry,after);
}
