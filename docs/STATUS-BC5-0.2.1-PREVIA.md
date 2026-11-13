# Prévia 0.2.1 — 04/11/2026

Prévia local: `dist/preview/AnyCompare-0.2.1.exe`.
Tamanho: 24.581.632 bytes. SHA-256:
`2ebd6ce54fddba73a1c4726cf4639631dfc6191ff6bc1c8f5e890fc38c2cdba0`.

O executável estável `dist/AnyCompare.exe` permanece 0.2.0, SHA-256
`7286c58242c959e0756cf29086bc134a20940d9d2b25ee4321abc231ab33ff6f`.
Nenhuma janela nova foi aberta nesta rodada, conforme preferência do usuário.
A prévia foi compilada e verificada por testes ocultos; a validação visual do
novo editor e a aprovação de paridade 1:1 continuam pendentes.

## Mudanças confirmadas em execução

- **Registro → Abrir Com → Editor de Texto**, em inglês
  **Registry → Open With → Text Editor**. A opção abre uma sessão interna de
  arquivo único, conserva a comparação e atua sobre o lado clicado. Não executa
  o aplicativo associado a `.reg` nem importa valores no Registro do Windows.
- Editor com abrir, salvar/salvar como, desfazer/refazer, seleção, copiar,
  recortar/colar, excluir, confirmação de alterações não salvas e proteção
  contra alterações externas no disco. Usa `Document` e os diálogos Windows
  existentes. Salvar permanece desabilitado enquanto o documento estiver limpo.
- Preservação de UTF-16/BOM/CRLF no cenário `.reg` testado. Cancelar a escolha de
  codificação ANSI não altera o buffer nem perde caracteres. Salvar avisa sobre
  Unicode incompatível e permite uma codificação sem perdas.
- Modo somente leitura permite selecionar/copiar e bloqueia mutações. O botão
  direito preserva a seleção: corrigido o caso em que Copiar retornava vazio.
- Salvar pelo editor atualiza as propriedades dos arquivos correspondentes nas
  sessões de Registro/Versões/Mídia já abertas. Abrir novamente o mesmo arquivo
  ativa a aba existente e conserva edições ainda não salvas.
- Base do editor disponível pela Home, Nova Sessão, histórico/espaço de trabalho
  e `--edit <arquivo>`. O armazenamento legado de sessões usa o mesmo caminho
  nos dois campos; a interface e o buffer são de arquivo único.
- Quebra automática de linha opcional **no editor independente**. O teste
  confere que há mais linhas visuais sem inserir `\n` no documento.
  **Word Wrap de Comparar Textos permanece pendente.**
- WAV/Opus passam à sessão de Mídia. Nos arquivos autorizados e nos relatórios
  controlados do BC5, esses formatos exibem apenas Áudio/Dados; o adaptador
  reproduz isso sem inventar tags do perfil estendido. Erros de parse não são
  convertidos silenciosamente em sucesso.
- Corrigidos os campos de encoder: MP3/ID3 distingue configurações de codificação
  de codificador; M4A exibe Codificador, Data de gravação e Comentário na ordem
  observada. Tamanhos de Dados usam agrupamento numérico PT/EN.

## Verificação

- **162 testes automáticos passaram; nove ignorados na suíte normal.**
- Dois testes ignorados foram executados isoladamente para inglês: conferem os
  textos efetivamente emitidos pelo renderizador de interface, inclusive o
  submenu Text Editor e Word Wrap. Encontraram e corrigiram os rótulos Undo,
  Redo, Cut, Paste e Select All que ainda apareciam em português.
- Um teste ignorado de mídia real passou em release, sobre dois arquivos por
  grupo (Opus usa o mesmo arquivo dos dois lados). Os oito carregamentos de
  M4A/MP3/WAV/Opus batem com os tamanhos de Dados dos relatórios nativos. SHA-256,
  tamanho e caminho foram verificados antes/depois; nenhuma mídia foi alterada.
- `cargo clippy --all-targets --locked -- -D warnings`: sem avisos.
- `cargo build --release --locked --target-dir .codex/build-021`: aprovado.
- Cópia da prévia conferida por SHA-256; executável estável intacto.

Logs locais em `.codex/bc5-ui-audit/`: `tests-divergences-04.log`,
`english-hidden-04.log`, `real-media-native-match-04.log`,
`clippy-divergences-04.log`, `build-divergences-04.log`.

Referências: ajuda original extraída `commandsreg.html` e `viewtextedit.html`,
imagem `shot_textedit.png`; relatórios nativos isolados
`user-{m4a,mp3,wav,opus}-native.txt`. As fontes de mídia do usuário não foram
incluídas no repositório. O pacote ETAPA24 foi aproveitado como leitores,
preservando a interface atual do projeto.

## Limitações e próxima ordem

**Base parcial, não equivalência completa.** O editor ainda precisa de pesquisa,
substituição, regras de sintaxe, seleção de fontes e demais comandos do original;
Sintaxe/Fonte ficam desabilitados. Geometria, fontes, detalhes de rolagem,
ClearType e DPI precisam de comparação visual no mesmo tamanho/estado.
O teste oculto não comprova pixels nem renderização final na GPU. O arquivo
`validation/0.2.1/release-smoke.json` anterior pertence a outro hash e não é
evidência de captura desta prévia.

Continuam pendentes reprodução de mídia, política completa de tags/formatos,
escrita e operações de Registro, Explorer nativo completo, Word Wrap em Comparar
Textos, mesclagem/sincronização/imagens e os demais itens de `BC5-PARIDADE.md`.
Não considerar o BC5 concluído nem reutilizar os números de recursos como prova.

Próximos passos: completar pesquisa/substituição no editor; revisar seleção e
rolagem com arquivos grandes; concluir auditoria visual quando houver autorização
para janelas; seguir a matriz de comandos e modos ainda ausentes. Manter os testes
sem janelas visíveis enquanto a preferência do usuário não mudar.
