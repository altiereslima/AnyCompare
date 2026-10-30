use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionKind {
    Text,
    Folder,
    Hex,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecentSession {
    pub kind: SessionKind,
    pub left: PathBuf,
    pub right: PathBuf,
    /// When the session was auto-saved (Unix seconds); 0 in histories written before
    /// 0.1.9, which take the history file's date on load. Ignored by equality.
    #[serde(default)]
    pub saved: i64,
}

impl PartialEq for RecentSession {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.left == other.left && self.right == other.right
    }
}

impl Eq for RecentSession {}

impl RecentSession {
    pub fn new(kind: SessionKind, left: PathBuf, right: PathBuf) -> Self {
        Self {
            kind,
            left,
            right,
            saved: now(),
        }
    }

    /// "left <--> right" with file names, as the original session titles.
    pub fn label(&self) -> String {
        let name = |path: &PathBuf| {
            path.file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
                .into_owned()
        };
        format!("{} <--> {}", name(&self.left), name(&self.right))
    }
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

#[derive(Serialize, Deserialize)]
pub struct History {
    pub version: u32,
    pub entries: Vec<RecentSession>,
    #[serde(default)]
    pub searches: Vec<String>,
}

impl Default for History {
    fn default() -> Self {
        Self {
            version: 1,
            entries: Vec::new(),
            searches: Vec::new(),
        }
    }
}

impl History {
    fn path() -> Option<PathBuf> {
        std::env::var_os("APPDATA")
            .map(|dir| PathBuf::from(dir).join("AnyCompare").join("sessions.json"))
    }

    pub fn load() -> Result<Self, String> {
        let Some(path) = Self::path() else {
            return Ok(Self::default());
        };
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default())
            }
            Err(error) => return Err(format!("Não foi possível ler o histórico: {error}")),
        };
        let mut history: Self = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Histórico inválido: {error}"))?;
        if history.version != 1 {
            return Err("O histórico tem uma versão não suportada.".into());
        }
        // Entries saved before dates were recorded: the file's own date is the best bound.
        let file_date = fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs() as i64);
        for entry in &mut history.entries {
            if entry.saved == 0 {
                entry.saved = file_date;
            }
        }
        Ok(history)
    }

    pub fn remember(&mut self, mut session: RecentSession) -> bool {
        session.saved = now();
        if self.entries.first() == Some(&session) {
            return false;
        }
        self.entries.retain(|entry| *entry != session);
        self.entries.insert(0, session);
        self.entries.truncate(30);
        true
    }

    pub fn remember_search(&mut self, query: String) -> bool {
        if query.is_empty() || query.len() > 4096 || self.searches.first() == Some(&query) {
            return false;
        }
        self.searches.retain(|entry| *entry != query);
        self.searches.insert(0, query);
        self.searches.truncate(30);
        true
    }

    pub fn text_paths(&self) -> Vec<String> {
        let mut paths = Vec::new();
        for entry in self
            .entries
            .iter()
            .filter(|entry| entry.kind == SessionKind::Text)
        {
            for path in [&entry.left, &entry.right] {
                let path = path.to_string_lossy().into_owned();
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }
        paths
    }

    pub fn forget(&mut self, session: &RecentSession) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| entry != session);
        self.entries.len() != before
    }

    pub fn save(&self) -> Result<(), String> {
        let Some(path) = Self::path() else {
            return Err("A pasta de dados do usuário não está disponível.".into());
        };
        fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
        fs::rename(&temporary, path)
            .map_err(|e| format!("Não foi possível salvar o histórico: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_roundtrips_unicode_paths_and_deduplicates() {
        let session = RecentSession::new(
            SessionKind::Text,
            "C:/pasta/ação.txt".into(),
            "D:/ação.txt".into(),
        );
        let mut history = History::default();
        assert!(history.remember(session.clone()));
        assert!(!history.remember(session.clone()));
        let restored: History =
            serde_json::from_str(&serde_json::to_string(&history).unwrap()).unwrap();
        assert_eq!(restored.entries, vec![session]);
        assert!(history.remember_search("Português".into()));
        assert!(history.remember_search("ação".into()));
        assert!(history.remember_search("Português".into()));
        assert!(!history.remember_search("Português".into()));
        assert_eq!(history.searches, ["Português", "ação"]);
        let restored: History =
            serde_json::from_str(&serde_json::to_string(&history).unwrap()).unwrap();
        assert_eq!(restored.searches, history.searches);
        let legacy: History = serde_json::from_str(r#"{"version":1,"entries":[]}"#).unwrap();
        assert!(legacy.searches.is_empty());
    }
}
