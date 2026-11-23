//! Snapshot de proveniência do arquivo de mídia para exportações e coleta A/B.
//!
//! Este SHA-256 NÃO é um campo de comparação do BC5. É um controle externo:
//! impede usar metadados em cache após alteração silenciosa dos bytes de áudio.
//! A varredura é em blocos, sem carregar o arquivo inteiro em memória.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFingerprint {
    /// Caminho canônico: detecta também troca do destino de links simbólicos.
    pub canonical_path: PathBuf,
    pub file_size: u64,
    pub sha256: String,
}

impl SourceFingerprint {
    /// Uma captura não prova autoria/algoritmo do BC5; apenas vincula os bytes
    /// à observação. Confere tamanho antes/depois da leitura para rejeitar
    /// crescimento ou truncamento durante a medição.
    pub fn capture(path: &Path) -> io::Result<Self> {
        let canonical_path = path.canonicalize()?;
        let mut file = File::open(&canonical_path)?;
        let expected_size = file.metadata()?.len();
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 64 * 1024];
        let mut read_total: u64 = 0;
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            read_total = read_total.checked_add(n as u64).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "overflow no tamanho da mídia")
            })?;
            hasher.update(&buf[..n]);
        }
        let after_size = file.metadata()?.len();
        if expected_size != read_total || expected_size != after_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "arquivo mudou de tamanho durante a captura",
            ));
        }
        Ok(Self {
            canonical_path,
            file_size: read_total,
            sha256: format!("{:x}", hasher.finalize()),
        })
    }

    /// Reabre e relê a origem carregada: não confia apenas na data de alteração
    /// nem na leitura de tags, que pode produzir o mesmo resultado após um patch.
    pub fn verify_current(&self, original_path: &Path) -> io::Result<()> {
        let now = Self::capture(original_path)?;
        if self != &now {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "bytes, tamanho ou caminho de origem mudaram desde o carregamento",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn same_length_audio_mutation_invalidates_cached_observation() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("x.mp3");
        fs::write(&file, b"ID3-AAAA-original-sample").unwrap();
        let guard = SourceFingerprint::capture(&file).unwrap();
        assert!(guard.verify_current(&file).is_ok());
        fs::write(&file, b"ID3-BBBB-original-sample").unwrap();
        assert_eq!(guard.file_size, fs::metadata(&file).unwrap().len());
        assert!(guard.verify_current(&file).is_err());
    }

    #[test]
    fn deleted_or_truncated_original_cannot_be_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.flac");
        fs::write(&file, b"fLaC\0data").unwrap();
        let guard = SourceFingerprint::capture(&file).unwrap();
        fs::write(&file, b"fLaC").unwrap();
        assert!(guard.verify_current(&file).is_err());
        fs::remove_file(&file).unwrap();
        assert!(guard.verify_current(&file).is_err());
    }

    #[test]
    fn large_streamed_input_keeps_same_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("stream.m4a");
        fs::write(&file, vec![42u8; 300_000]).unwrap();
        let guard = SourceFingerprint::capture(&file).unwrap();
        assert_eq!(guard.file_size, 300_000);
        assert_eq!(guard.sha256.len(), 64);
        assert!(guard.verify_current(&file).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn swapped_symlink_target_is_detected_even_with_identical_bytes() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let (a, b, link) = (
            dir.path().join("a"),
            dir.path().join("b"),
            dir.path().join("link"),
        );
        fs::write(&a, b"content").unwrap();
        fs::write(&b, b"content").unwrap();
        symlink(&a, &link).unwrap();
        let guard = SourceFingerprint::capture(&link).unwrap();
        fs::remove_file(&link).unwrap();
        symlink(&b, &link).unwrap();
        assert!(guard.verify_current(&link).is_err());
    }
}
