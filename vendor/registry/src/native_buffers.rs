//! Crescimento dos buffers de RegEnumValueW, isolado da API Win32.
//!
//! Em ERROR_MORE_DATA, `lpcchValueName` pode ser **indefinido**, segundo a
//! documentação Win32. Não o usamos para determinar o tamanho seguinte.
//! `lpcbData` pode informar o necessário quando o buffer de dados é pequeno;
//! caso não ofereça informação útil, tentamos o nome e depois os dados, com
//! limites rígidos. Os testes deste módulo são independentes do Windows.

pub(crate) const MAX_NAME_UNITS: usize = 32768;
pub(crate) const MAX_VALUE_BYTES: usize = 16 * 1024 * 1024;

fn grow(old: usize, needed: usize, max: usize) -> Result<usize, String> {
    let next = old.saturating_mul(2).max(needed).min(max);
    if next <= old {
        return Err("buffer do Registro não pode crescer".into());
    }
    Ok(next)
}

/// Capacidades em caracteres UTF-16 (`name_cap`) e bytes (`data_cap`).
/// Jamais consulta `lpcchValueName` após ERROR_MORE_DATA: esse valor pode
/// estar indefinido. `reported_data` é utilizado apenas como hint limitado.
pub(crate) fn next_value_buffers(
    name_cap: usize,
    data_cap: usize,
    reported_data: u32,
) -> Result<(usize, usize), String> {
    if name_cap == 0 || data_cap == 0 || name_cap > MAX_NAME_UNITS || data_cap > MAX_VALUE_BYTES {
        return Err("capacidades de Registro inválidas".into());
    }
    let required = reported_data as usize;
    if required > data_cap && required <= MAX_VALUE_BYTES {
        // Caso comum: somente os bytes de um valor grande não cabem.
        return Ok((name_cap, grow(data_cap, required, MAX_VALUE_BYTES)?));
    }
    if name_cap < MAX_NAME_UNITS {
        // Se só o nome era pequeno, a API não garante o tamanho necessário.
        return Ok((grow(name_cap, name_cap + 1, MAX_NAME_UNITS)?, data_cap));
    }
    if data_cap < MAX_VALUE_BYTES {
        // A API pode não informar corretamente o tamanho durante a falha.
        return Ok((name_cap, grow(data_cap, data_cap + 1, MAX_VALUE_BYTES)?));
    }
    Err("ERROR_MORE_DATA persistiu com buffers nos limites".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn large_binary_with_reported_size_does_not_grow_name() {
        let (name, data) = next_value_buffers(256, 1024, 100_000).unwrap();
        assert_eq!(name, 256);
        assert_eq!(data, 100_000);
    }
    #[test]
    fn small_name_buffer_grows_without_relying_on_undefined_length() {
        let (name, data) = next_value_buffers(256, 1024, 100).unwrap();
        assert_eq!((name, data), (512, 1024));
    }
    #[test]
    fn data_cannot_exceed_limit_even_when_size_hint_is_unusable() {
        let (name, data) = next_value_buffers(256, 1024, u32::MAX).unwrap();
        assert_eq!((name, data), (512, 1024));
        assert!(next_value_buffers(MAX_NAME_UNITS, MAX_VALUE_BYTES, u32::MAX).is_err());
    }
    #[test]
    fn fallback_grows_data_after_name_cap_reached() {
        assert_eq!(
            next_value_buffers(MAX_NAME_UNITS, 1024, 0).unwrap(),
            (MAX_NAME_UNITS, 2048)
        );
    }
    #[test]
    fn invalid_input_is_rejected() {
        assert!(next_value_buffers(0, 1024, 0).is_err());
        assert!(next_value_buffers(256, 0, 0).is_err());
    }
}
