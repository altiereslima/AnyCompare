//! Destaque intravalor para Registry Compare (BC5 documenta destaque por caractere).
//!
//! Este algoritmo de prefixo/sufixo comum é um **protótipo**, não uma
//! reprodução comprovada do algoritmo visual do BC5. Não altera a semântica
//! de igualdade (tipo + bytes) da comparação.
use super::RegValue;

pub const INLINE_PREVIEW_CHARS: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharacterDifference {
    pub common_prefix: String,
    pub left_changed: String,
    pub right_changed: String,
    pub common_suffix: String,
}

/// Retorna as regiões distintas da representação legível. O valor original
/// continua sendo comparado pelos bytes; não aplica normalização ou tolerância.
/// Não tenta produzir um alinhamento ótimo, pois ele ainda depende de A/B.
pub fn character_difference(left: &RegValue, right: &RegValue) -> Option<CharacterDifference> {
    if left.type_id != right.type_id || left.bytes == right.bytes {
        return None;
    }
    let a = left.displayed_preview(INLINE_PREVIEW_CHARS);
    let b = right.displayed_preview(INLINE_PREVIEW_CHARS);
    if a == b {
        return None;
    } // Pode haver diferenças após o limite do preview.
    let ac: Vec<char> = a.chars().collect();
    let bc: Vec<char> = b.chars().collect();
    let mut prefix = 0usize;
    while prefix < ac.len() && prefix < bc.len() && ac[prefix] == bc[prefix] {
        prefix += 1;
    }
    let mut suffix = 0usize;
    while suffix < ac.len() - prefix
        && suffix < bc.len() - prefix
        && ac[ac.len() - suffix - 1] == bc[bc.len() - suffix - 1]
    {
        suffix += 1;
    }
    Some(CharacterDifference {
        common_prefix: ac[..prefix].iter().collect(),
        left_changed: ac[prefix..ac.len() - suffix].iter().collect(),
        right_changed: bc[prefix..bc.len() - suffix].iter().collect(),
        common_suffix: ac[ac.len() - suffix..].iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn text(s: &str) -> RegValue {
        let mut bytes = Vec::new();
        for u in s.encode_utf16().chain(std::iter::once(0)) {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        RegValue {
            name: "Text".into(),
            type_id: 1,
            bytes,
        }
    }
    #[test]
    fn unicode_difference_with_common_prefix_and_suffix() {
        let d = character_difference(&text("Ação 🐱 final"), &text("Ação 🐶 final")).unwrap();
        assert_eq!(d.common_prefix, "Ação ");
        assert_eq!(d.left_changed, "🐱");
        assert_eq!(d.right_changed, "🐶");
        assert_eq!(d.common_suffix, " final");
    }
    #[test]
    fn same_bytes_and_type_need_no_highlight() {
        assert!(character_difference(&text("ABC"), &text("ABC")).is_none());
    }
    #[test]
    fn changed_type_is_not_misrepresented_as_character_difference() {
        let a = text("ABC");
        let mut b = a.clone();
        b.type_id = 2;
        assert!(character_difference(&a, &b).is_none());
    }
    #[test]
    fn differing_binary_bytes_show_different_hex_characters() {
        let a = RegValue {
            name: "B".into(),
            type_id: 3,
            bytes: vec![0x10, 0x20, 0x30],
        };
        let b = RegValue {
            name: "B".into(),
            type_id: 3,
            bytes: vec![0x10, 0x2F, 0x30],
        };
        let d = character_difference(&a, &b).unwrap();
        assert_eq!(d.left_changed, "0");
        assert_eq!(d.right_changed, "F");
    }
    #[test]
    fn escaped_preview_is_bounded_for_large_binary_values() {
        let a = RegValue {
            name: "Huge".into(),
            type_id: 3,
            bytes: vec![0xaa; 4 * 1024 * 1024],
        };
        let preview = a.displayed_preview(64);
        assert!(preview.chars().count() < 200);
        assert!(preview.contains("4194304 bytes"));
    }
}
