//! Editor transacional de snapshots em memória, SEM escrita no Registro vivo.
//!
//! Os nomes das operações provêm de TREGACTIONS e da ajuda do BC5, mas esta
//! implementação é uma camada de testes: NÃO representa paridade A/B validada.
use super::{canonical, RegKey, RegValue, Registry};
use std::collections::BTreeMap;

const HISTORY_LIMIT: usize = 8;
const MAX_EDIT_SNAPSHOT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct SnapshotEditor {
    pub registry: Registry,
    undo_stack: Vec<Registry>,
    redo_stack: Vec<Registry>,
}

fn storage_size(reg: &Registry) -> Result<usize, String> {
    let mut size = 0usize;
    for key in reg.keys.values() {
        size = size.checked_add(key.path.len()).ok_or("tamanho de snapshot excedido")?;
        for value in key.values.values() {
            size = size.checked_add(value.name.len()).and_then(|x| x.checked_add(value.bytes.len()))
                .ok_or("tamanho de snapshot excedido")?;
        }
    }
    Ok(size)
}
pub(crate) fn validate_snapshot(reg: &Registry) -> Result<(), String> {
    for (id, key) in &reg.keys {
        if !legal_path(&key.path) || canonical(&key.path) != *id {
            return Err("snapshot contém chave com identidade inconsistente".into());
        }
        // Um snapshot gerado pelo parser ou leitor nativo sempre possui os
        // ancestrais. Sem essa exigência, a exportação/cópia poderia ocultar
        // ramos desconectados do Registro.
        if let Some((parent, _)) = id.rsplit_once('\\') {
            if !reg.keys.contains_key(parent) {
                return Err(format!("snapshot possui chave sem ancestral: {}", key.path));
            }
        }
        for (value_id, value) in &key.values {
            if !legal_name(&value.name) || canonical(&value.name) != *value_id {
                return Err("snapshot contém valor com identidade inconsistente".into());
            }
        }
    }
    Ok(())
}
fn check_size(reg: &Registry) -> Result<(), String> {
    validate_snapshot(reg)?;
    if storage_size(reg)? > MAX_EDIT_SNAPSHOT_BYTES {
        return Err("snapshot acima de 8 MiB: edição em memória desabilitada".into());
    }
    Ok(())
}
fn legal_path(path: &str) -> bool {
    if path.is_empty() || path.contains('\0') || path.contains('/') || path.contains('\n') || path.contains('\r') ||
        path.starts_with('\\') || path.ends_with('\\') || path.split('\\').any(str::is_empty) { return false; }
    matches!(path.split('\\').next().unwrap_or("").to_ascii_uppercase().as_str(),
        "HKEY_CLASSES_ROOT" | "HKEY_CURRENT_USER" | "HKEY_LOCAL_MACHINE" | "HKEY_USERS" | "HKEY_CURRENT_CONFIG")
}
fn legal_name(name: &str) -> bool { !name.contains('\0') && !name.contains('\n') && !name.contains('\r') }

impl SnapshotEditor {
    pub fn new(registry: Registry) -> Result<Self, String> {
        check_size(&registry)?;
        Ok(Self {registry, undo_stack:Vec::new(), redo_stack:Vec::new()})
    }
    pub fn undo_count(&self) -> usize { self.undo_stack.len() }
    pub fn redo_count(&self) -> usize { self.redo_stack.len() }
    fn edit(&mut self, operation: impl FnOnce(&mut Registry) -> Result<(), String>) -> Result<(), String> {
        check_size(&self.registry)?;
        let mut candidate = self.registry.clone();
        operation(&mut candidate)?;
        check_size(&candidate)?;
        if candidate != self.registry {
            if self.undo_stack.len() == HISTORY_LIMIT {self.undo_stack.remove(0);}
            self.undo_stack.push(std::mem::replace(&mut self.registry, candidate));
            self.redo_stack.clear();
        }
        Ok(())
    }
    pub fn undo(&mut self) -> Result<(), String> {
        let previous = self.undo_stack.pop().ok_or("não há alteração para desfazer")?;
        self.redo_stack.push(std::mem::replace(&mut self.registry, previous));
        Ok(())
    }
    pub fn redo(&mut self) -> Result<(), String> {
        let next = self.redo_stack.pop().ok_or("não há alteração para refazer")?;
        self.undo_stack.push(std::mem::replace(&mut self.registry, next));
        Ok(())
    }
    pub fn new_key(&mut self, path: &str) -> Result<(), String> {
        if !legal_path(path) {return Err("caminho de chave inválido".into());}
        let (parent, _) = path.rsplit_once('\\').ok_or("não é permitido criar raiz")?;
        let id = canonical(path);
        self.edit(|reg| {
            if !reg.keys.contains_key(&canonical(parent)) {return Err("chave pai ausente".into());}
            if reg.keys.contains_key(&id) {return Err("chave já existe".into());}
            reg.keys.insert(id, RegKey{path:path.into(),values:BTreeMap::new()});
            Ok(())
        })
    }
    pub fn delete_key(&mut self, path: &str) -> Result<(), String> {
        if !legal_path(path) || !path.contains('\\') {return Err("não é permitido excluir a raiz".into());}
        let id = canonical(path);
        self.edit(|reg| {
            if !reg.keys.contains_key(&id) {return Err("chave ausente".into());}
            let prefix = format!("{id}\\");
            reg.keys.retain(|key,_| key != &id && !key.starts_with(&prefix));
            Ok(())
        })
    }
    pub fn rename_key(&mut self, path: &str, new_name: &str) -> Result<(), String> {
        if !legal_path(path) || !path.contains('\\') {return Err("não é permitido renomear a raiz".into());}
        if new_name.is_empty() || new_name.contains('\\') || !legal_name(new_name) || new_name.contains('/') {
            return Err("novo nome de chave inválido".into());
        }
        let (parent, _) = path.rsplit_once('\\').ok_or("chave sem pai")?;
        let id = canonical(path);
        let destination = format!("{parent}\\{new_name}");
        self.edit(|reg| {
            if !reg.keys.contains_key(&id) {return Err("chave ausente".into());}
            let prefix = format!("{id}\\");
            let selected: Vec<String> = reg.keys.keys().filter(|k| *k == &id || k.starts_with(&prefix)).cloned().collect();
            let component_count = reg.keys[&id].path.split('\\').count();
            let mut renamed = Vec::new();
            for old_id in &selected {
                // Não use path.len() de uma grafia fornecida pelo usuário: nomes
                // Unicode com conversão de caixa podem mudar o tamanho em bytes.
                let suffix:Vec<&str> = reg.keys[old_id].path.split('\\').skip(component_count).collect();
                let new_path = if suffix.is_empty() {destination.clone()}
                    else {format!("{destination}\\{}",suffix.join("\\"))};
                let new_id = canonical(&new_path);
                if reg.keys.contains_key(&new_id) && !selected.contains(&new_id) {return Err("destino já existe".into());}
                renamed.push((old_id.clone(),new_id,new_path));
            }
            // Evita colisão entre entradas alteradas, inclusive com caixa diferente.
            let mut check = std::collections::BTreeSet::new();
            if renamed.iter().any(|(_,id,_)| !check.insert(id.clone())) {return Err("colisão de chaves".into());}
            let mut updates = Vec::new();
            for (old_id,new_id,new_path) in renamed {
                let mut key = reg.keys.remove(&old_id).ok_or("chave desapareceu")?;
                key.path = new_path;
                updates.push((new_id,key));
            }
            reg.keys.extend(updates);
            Ok(())
        })
    }
    pub fn new_value(&mut self, key: &str, value: RegValue) -> Result<(), String> {
        self.set_value_impl(key,value,false)
    }
    pub fn modify_value(&mut self, key: &str, value: RegValue) -> Result<(), String> {
        self.set_value_impl(key,value,true)
    }
    fn set_value_impl(&mut self, key: &str, value: RegValue, must_exist: bool) -> Result<(), String> {
        if !legal_path(key) || !legal_name(&value.name) {return Err("nome ou chave inválidos".into());}
        let id = canonical(key);
        self.edit(move |reg| {
            let values = &mut reg.keys.get_mut(&id).ok_or("chave ausente")?.values;
            let name_id = canonical(&value.name);
            if values.contains_key(&name_id) != must_exist {
                return Err(if must_exist {"valor ausente"} else {"valor já existe"}.into());
            }
            values.insert(name_id,value);
            Ok(())
        })
    }
    pub fn delete_value(&mut self, key: &str, name: &str) -> Result<(), String> {
        if !legal_path(key) || !legal_name(name) {return Err("nome ou chave inválidos".into());}
        self.edit(|reg| {
            let values = &mut reg.keys.get_mut(&canonical(key)).ok_or("chave ausente")?.values;
            values.remove(&canonical(name)).ok_or("valor ausente")?;
            Ok(())
        })
    }
    pub fn rename_value(&mut self, key: &str, name: &str, new_name: &str) -> Result<(), String> {
        if !legal_path(key) || !legal_name(name) || !legal_name(new_name) {return Err("nome ou chave inválidos".into());}
        self.edit(|reg| {
            let values = &mut reg.keys.get_mut(&canonical(key)).ok_or("chave ausente")?.values;
            let old_id=canonical(name);
            let new_id=canonical(new_name);
            if !values.contains_key(&old_id) {return Err("valor ausente".into());}
            if new_id != old_id && values.contains_key(&new_id) {return Err("destino já existe".into());}
            let mut value=values.remove(&old_id).ok_or("valor ausente")?;
            value.name=new_name.to_string();
            values.insert(new_id,value);
            Ok(())
        })
    }
    /// Copia uma chave e todos os descendentes entre snapshots exclusivamente em
    /// memória. O destino é `parent\new_name`; somente copia para locais
    /// ainda ausentes. NÃO equivale à política de sobrescrita original do BC5.
    /// A operação é atômica; inclusive subchaves, tipos e bytes são revertidos
    /// integralmente por undo.
    pub fn copy_key_from(
        &mut self,
        source: &Registry,
        source_key: &str,
        target_parent: &str,
        new_name: &str,
    ) -> Result<(), String> {
        if !legal_path(source_key) || !source_key.contains('\\') ||
            !legal_path(target_parent) || !legal_name(new_name) ||
            new_name.is_empty() || new_name.contains('\\') || new_name.contains('/') {
            return Err("origem, pai ou novo nome inválido para cópia de chave".into());
        }
        validate_snapshot(source)?;
        if storage_size(source)? > MAX_EDIT_SNAPSHOT_BYTES {
            return Err("snapshot de origem acima de 8 MiB para cópia".into());
        }
        let source_id = canonical(source_key);
        let root = source.keys.get(&source_id).ok_or("chave de origem ausente")?;
        let component_count = root.path.split('\\').count();
        let prefix = format!("{source_id}\\");
        let mut relative_paths: Vec<(Vec<String>, BTreeMap<String, RegValue>)> = Vec::new();
        for (id, key) in &source.keys {
            if id != &source_id && !id.starts_with(&prefix) { continue; }
            let components = key.path.split('\\').skip(component_count)
                .map(str::to_owned).collect::<Vec<_>>();
            relative_paths.push((components, key.values.clone()));
        }
        if relative_paths.is_empty() { return Err("subárvore de origem vazia".into()); }
        let parent_id = canonical(target_parent);
        let new_name = new_name.to_owned();
        self.edit(move |reg| {
            let actual_parent = reg.keys.get(&parent_id)
                .ok_or("chave pai do destino ausente")?.path.clone();
            let dest_root = format!("{actual_parent}\\{new_name}");
            let mut staged: Vec<(String, RegKey)> = Vec::new();
            let mut generated = std::collections::BTreeSet::new();
            for (relative, values) in relative_paths {
                let path = if relative.is_empty() { dest_root.clone() }
                    else { format!("{dest_root}\\{}", relative.join("\\")) };
                if !legal_path(&path) { return Err("caminho de cópia inválido".into()); }
                let id = canonical(&path);
                if reg.keys.contains_key(&id) || !generated.insert(id.clone()) {
                    return Err("destino de cópia já existe ou possui colisão".into());
                }
                staged.push((id, RegKey {path, values}));
            }
            for (id, key) in staged { reg.keys.insert(id, key); }
            Ok(())
        })
    }

    /// Copia uma subárvore para a mesma posição de outro snapshot quando ela
    /// ainda não existe no destino. Sem mescla nem sobrescrita silenciosa.
    pub fn copy_missing_key_from(&mut self, source: &Registry, source_key: &str)
        -> Result<(), String> {
        let (parent, component) = source_key.rsplit_once('\\')
            .ok_or("copiar uma chave raiz não é permitido")?;
        self.copy_key_from(source, source_key, parent, component)
    }

    /// Copia um valor bruto entre snapshots, sem sobrescrever destino existente.
    /// A política de conflito original do BC5 ainda precisa ser obtida por A/B.
    pub fn copy_value_from(&mut self, source: &Registry, from_key: &str, value_name: &str, to_key: &str) -> Result<(), String> {
        if !legal_path(from_key) || !legal_path(to_key) || !legal_name(value_name) {
            return Err("nome ou chave inválidos".into());
        }
        validate_snapshot(source)?;
        if storage_size(source)? > MAX_EDIT_SNAPSHOT_BYTES {
            return Err("snapshot de origem acima de 8 MiB para cópia".into());
        }
        let value = source.keys.get(&canonical(from_key)).and_then(|k|k.values.get(&canonical(value_name)))
            .cloned().ok_or("valor de origem ausente")?;
        self.new_value(to_key,value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{compare, parse_reg, Difference};
    fn base() -> Registry {
        parse_reg(b"Windows Registry Editor Version 5.00\n[HKEY_CURRENT_USER\\Demo]\n\"N\"=dword:00000001\n[HKEY_CURRENT_USER\\Demo\\Child]\n\"Other\"=hex:de,ad\n").unwrap()
    }
    #[test]
    fn edit_modify_then_undo_and_redo_preserves_bytes() {
        let initial=base();
        let mut edit=SnapshotEditor::new(initial.clone()).unwrap();
        let value=RegValue{name:"N".into(),type_id:7,bytes:vec![0,0,0,0]};
        edit.modify_value("HKEY_CURRENT_USER\\Demo",value).unwrap();
        assert_ne!(edit.registry,initial);
        edit.undo().unwrap();assert_eq!(edit.registry,initial);
        edit.redo().unwrap();assert_eq!(edit.registry.keys["hkey_current_user\\demo"].values["n"].type_id,7);
    }
    #[test]
    fn rename_subtree_is_atomic_and_case_aware() {
        let mut edit=SnapshotEditor::new(base()).unwrap();
        edit.rename_key("HKEY_CURRENT_USER\\Demo","Moved").unwrap();
        assert!(edit.registry.keys.contains_key("hkey_current_user\\moved\\child"));
        assert!(!edit.registry.keys.contains_key("hkey_current_user\\demo"));
        edit.undo().unwrap();
        assert!(edit.registry.keys.contains_key("hkey_current_user\\demo\\child"));
    }
    #[test]
    fn root_delete_or_collision_does_not_mutate_snapshot() {
        let mut edit=SnapshotEditor::new(base()).unwrap();
        let before=edit.registry.clone();
        assert!(edit.delete_key("HKEY_CURRENT_USER").is_err());
        assert!(edit.new_key("HKEY_CURRENT_USER\\Demo").is_err());
        assert!(edit.rename_key("HKEY_CURRENT_USER\\Demo","Demo").is_ok());
        assert_eq!(edit.registry,before);
        assert_eq!(edit.undo_count(),0);
    }
    #[test]
    fn copy_does_not_overwrite_and_raw_data_roundtrips() {
        let source=base();
        let mut target=SnapshotEditor::new(base()).unwrap();
        assert!(target.copy_value_from(&source,"HKEY_CURRENT_USER\\Demo","N","HKEY_CURRENT_USER\\Demo").is_err());
        target.copy_value_from(&source,"HKEY_CURRENT_USER\\Demo","N","HKEY_CURRENT_USER\\Demo\\Child").unwrap();
        assert_eq!(target.registry.keys["hkey_current_user\\demo\\child"].values["n"].bytes,[1,0,0,0]);
        let diff=compare(&source,&target.registry);
        assert!(diff.rows.iter().any(|r|r.status==Difference::Changed));
    }
    #[test]
    fn value_name_can_contain_backslash_and_default_is_empty() {
        let mut edit=SnapshotEditor::new(base()).unwrap();
        edit.new_value("HKEY_CURRENT_USER\\Demo",RegValue{name:"A\\B".into(),type_id:3,bytes:vec![255]}).unwrap();
        edit.new_value("HKEY_CURRENT_USER\\Demo",RegValue{name:String::new(),type_id:1,bytes:vec![0,0]}).unwrap();
        edit.rename_value("HKEY_CURRENT_USER\\Demo","A\\B","C\\D").unwrap();
        edit.delete_value("HKEY_CURRENT_USER\\Demo","").unwrap();
        let values=&edit.registry.keys["hkey_current_user\\demo"].values;
        assert!(values.contains_key("c\\d"));assert!(!values.contains_key(""));
    }
    #[test]
    fn redo_stack_clears_on_new_change() {
        let mut edit=SnapshotEditor::new(base()).unwrap();
        edit.delete_value("HKEY_CURRENT_USER\\Demo","N").unwrap();
        edit.undo().unwrap();assert_eq!(edit.redo_count(),1);
        edit.new_key("HKEY_CURRENT_USER\\Demo\\OtherKey").unwrap();
        assert_eq!(edit.redo_count(),0);
    }
    #[test]
    fn forged_snapshot_is_rejected_before_any_edit() {
        let mut malformed=base();
        malformed.keys.get_mut("hkey_current_user\\demo").unwrap().path="HKEY_CURRENT_USER\\Wrong".into();
        assert!(SnapshotEditor::new(malformed).is_err());
    }
    #[test]
    fn unicode_case_insensitive_rename_must_not_slice_user_bytes() {
        let mut editor=SnapshotEditor::new(base()).unwrap();
        editor.rename_key("hkey_current_user\\demo","Renamed").unwrap();
        assert!(editor.registry.keys.contains_key("hkey_current_user\\renamed\\child"));
    }

}
