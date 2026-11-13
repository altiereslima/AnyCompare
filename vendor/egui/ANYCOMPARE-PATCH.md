# Patch do AnyCompare sobre egui 0.29.1

Fonte: crate egui 0.29.1 do cache Cargo (licenças MIT/Apache-2.0 do upstream).
Aplicado por `[patch.crates-io]` no Cargo.toml. Cada mudança está marcada com
"AnyCompare" no código. Motivo comum: documentos de dezenas de milhares de
linhas (medido com 57.108 linhas por lado em 13/11/2026).

- `widgets/text_edit/builder.rs`
  - `TextEdit::disable_undoer()`: o app tem desfazer próprio; o undoer interno
    clonava e comparava o buffer inteiro duas vezes por quadro com foco.
  - O texto anterior só é clonado com AccessKit ativo; o valor acessível de
    buffers acima de 64 KiB fica vazio (`accessible_value`).
  - Seleção pintada como retângulos atrás do galley
    (`paint_text_selection_shapes`), sem clonar um galley compartilhado.
  - O galley anterior é liberado antes do relayout de uma edição, permitindo
    que o layouter atualize o galley do documento no lugar.
- `text_selection/accesskit_text.rs`: só as linhas visíveis (e as que contêm as
  pontas do cursor) viram nós. Com qualquer cliente de UI Automation ativo, o
  egui recriava um nó por linha a cada quadro: ~0,5 s por passo de rolagem.
- `text_selection/visuals.rs`: `paint_text_selection_shapes`.
- `text_selection/text_cursor_state.rs`: `byte_index_from_char_index` pula
  blocos de 4 KiB contando inícios de caractere.
- `context.rs`: `Context::accesskit_enabled()`.
- `lib.rs`: `#![allow(warnings)]` para avisos do rustc atual em código upstream.
