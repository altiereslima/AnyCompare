use crate::{document::atomic_write, history::RecentSession};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Serialize, Deserialize)]
pub struct SavedSession {
    pub files: RecentSession,
    #[serde(default)]
    pub options: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
pub struct Workspace {
    pub version: u32,
    pub sessions: Vec<SavedSession>,
    pub active: Option<usize>,
}

impl Workspace {
    pub fn new(sessions: Vec<SavedSession>, active: Option<usize>) -> Self {
        Self {
            version: 1,
            sessions,
            active,
        }
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() > 16 * 1024 * 1024 {
            return Err("O espaço de trabalho excede o tamanho permitido.".into());
        }
        let value: Self = serde_json::from_reader(file)
            .map_err(|e| format!("Espaço de trabalho inválido: {e}"))?;
        if value.version != 1
            || value.sessions.len() > 100
            || value.active.is_some_and(|i| i >= value.sessions.len())
        {
            return Err("Versão ou seleção de espaço de trabalho inválida.".into());
        }
        for session in &value.sessions {
            if !session.files.left.exists() || !session.files.right.exists() {
                return Err(format!(
                    "Os arquivos da sessão não foram encontrados: {} / {}",
                    session.files.left.display(),
                    session.files.right.display()
                ));
            }
            let valid = match session.files.kind {
                crate::history::SessionKind::Folder => {
                    session.files.left.is_dir() && session.files.right.is_dir()
                }
                _ => session.files.left.is_file() && session.files.right.is_file(),
            };
            if !valid {
                return Err("Os caminhos não correspondem ao tipo da sessão.".into());
            }
        }
        Ok(value)
    }
    pub fn save(&self, path: &Path, protected: &[PathBuf]) -> Result<(), String> {
        let key = |p: &Path| {
            p.canonicalize()
                .unwrap_or_else(|_| p.to_owned())
                .to_string_lossy()
                .to_lowercase()
        };
        if protected.iter().any(|p| key(p) == key(path)) {
            return Err("A sessão não pode substituir um arquivo comparado.".into());
        }
        atomic_write(
            path,
            &serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workspace_roundtrip_validates_inputs_and_never_overwrites_a_document() {
        let dir = std::env::temp_dir().join(format!(
            "anycompare-workspace-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        let input = dir.join("ação.txt");
        let output = dir.join("sessão.json");
        std::fs::write(&input, "conteúdo").unwrap();
        let session = SavedSession {
            files: RecentSession::new(
                crate::history::SessionKind::Text,
                input.clone(),
                input.clone(),
            ),
            options: serde_json::json!({"differences":true}),
        };
        let workspace = Workspace::new(vec![session], Some(0));
        assert!(workspace
            .save(&input, std::slice::from_ref(&input))
            .is_err());
        assert_eq!(std::fs::read_to_string(&input).unwrap(), "conteúdo");
        workspace
            .save(&output, std::slice::from_ref(&input))
            .unwrap();
        let loaded = Workspace::load(&output).unwrap();
        assert_eq!(loaded.sessions[0].files.left, input);
        assert_eq!(loaded.sessions[0].options["differences"], true);
        std::fs::write(&output, b"{\"version\":9,\"sessions\":[],\"active\":null}").unwrap();
        assert!(Workspace::load(&output).is_err());
        std::fs::remove_file(input).unwrap();
        std::fs::remove_file(output).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
