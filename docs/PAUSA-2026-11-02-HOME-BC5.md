# Pausa atual — Home BC5, 02/11/2026

O usuário pediu **“vamos pausar”**. Trabalho interrompido; somente este checkpoint foi registrado depois do pedido. Não continuar implementação, testes, capturas ou publicação sem retomada. Sem subagentes ou automações. Este checkpoint substitui o arquivo de pausa anterior `PAUSA-2026-11-02-BC5.md`, que descreve uma etapa já superada.

## Entrega preservada e fonte em desenvolvimento

- `dist/AnyCompare.exe` e `dist/AnyCompare-0.2.0.exe` permanecem a entrega validada: SHA-256 `7286c58242c959e0756cf29086bc134a20940d9d2b25ee4321abc231ab33ff6f`. Ver `STATUS-BC5-0.2.0.md` (136 testes e 81 capturas).
- Fonte elevada a **0.2.1**, ainda não entregue. Muitas alterações anteriores não commitadas já existiam: não reverter trabalho de outras etapas. Nenhum commit ou push realizado.
- Última suíte completa da fonte 0.2.1: **136 aprovados, zero falhas, 4 manuais ignorados**, log `.codex/bc5-ui-audit/tests-0.2.1.log`.
- A compilação release já iniciada terminou com sucesso durante o registro da pausa: log `.codex/bc5-ui-audit/build-0.2.1.log`. `target/release/anycompare.exe` contém 0.2.1 **sem validação visual**; não foi copiado para dist. Não houve captura visual ou clippy desta etapa.

## Alterações ainda sem validação visual

1. `src/ui/home.rs`: removida paginação; mostra os 12 tipos simultaneamente. Primeira faixa: Pastas, Combinar Pastas, Sincronizar Pastas, Textos, Mesclar Texto, Editar texto. Segunda: Hex, Mídia, Imagens, Registro, Tabelas, Versões. Apenas os cinco módulos implementados são ativados. Os sete ausentes continuam cinza, sem atalhos falsos. Lateral agora usa 20% da largura, menu branco. A posição vertical usa ajuste empírico a duas capturas, **não é um handler BC5 recuperado nem prova pixel a pixel**. Conferir também janelas menores/altura limitada. Conteúdo legado do popup Sessões foi preservado ao excluir Text Edit dessa árvore (evita regressão do teste 196×218); ainda difere da árvore Novo BC5 observada, que tem dez itens.
2. `tools/extract_bc5_home.py`: extrai somente HomeImages/HomeMediumImages das quatro escalas BC5, autenticando SHA do PE, recurso e blob. **112 variantes** em `assets/bc5/home`; catálogo `src/ui/bc5_home_icons.rs`, manifesto `docs/reference/bc5/home-icons.json`. BC4 assets preservados. `IconManager::home_icon` escolhe arte separada 64/32 px sem redimensionar uma para a outra. Text Edit passou a ter o ícone original. Desabilitado usa tratamento cinza existente.
3. Tabelas: faixa de visão geral estendida desde os caminhos até o fim dos detalhes, com viewport calculado pela altura da grade. Altura padrão dos detalhes 76→82, cabeçalho 26→27. Triângulo azul da coluna selecionada nos cabeçalhos da grade e detalhes. Seletores de metadata agora reservam/desenham seta no próprio botão de menu. **Ainda não há captura para confirmar geometria, cor, hit-testing e ausência de sobreposição**.
4. `assets/locales/en.json`: Mídia, Text Edit e dica BC5 em inglês.

## Evidência da fonte pelo REA

REA/Frida observou em execução os próprios desenhos de células da referência isolada: **Consolas, altura -13, peso 400, qualidade 0**; não trocar família por tentativa para corrigir o aspecto pesado. Evidência `ev_f72a07c21cfe2cce79e9aa4f0615262c2df71d15a175602cd5ccd57befbfd9c1`, exportada integralmente em `docs/reference/bc5/table-font-trace-2026-11-02.json`. A limitação de rasterização cinza versus ClearType RGB continua aberta. Não foi implementado renderizador RGB.

Sessão Frida efêmera já estava fechada quando consultada; a janela de referência própria PID 23524 recebeu CloseMainWindow antes da pausa e o processo não foi encontrado na última leitura. Nenhum processo do usuário ou REA/Codex foi encerrado.

## Retomada

- **Pendência acrescentada pelo usuário durante a pausa, para 03/11/2026:** em
  Comparar Tabelas, duplo clique deve selecionar a célula e iniciar edição na
  própria grade, com o conteúdo selecionado, conforme a imagem do BC5
  `C:/Users/Altieres/AppData/Local/Temp/codex-clipboard-0b7d71e0-d6e5-40d1-b93a-10ead20d3429.png`
  (Global.csv, B8). O usuário relata que isso não funciona no AnyCompare.
  Confirmar comportamento com REA/Craft e corrigir em fixtures; testar foco,
  seleção, confirmar/cancelar e ativação de Salvar. **Somente registrado, sem
  implementação ou testes após a pausa.** Item também em `BC5-PARIDADE.md`.

- Ler AGENTS e este checkpoint; conferir alterações existentes. Conferir build já iniciado, fmt/clippy e criar capturas antes de entregar 0.2.1. Priorizar comparação Home nas dimensões 1920/1153 e largura pequena, tabelas com as faixas/triângulos/setas novas. Não chamar layout empírico de 1:1.
- Finalizar `docs/validation/0.2.0/VALIDATION.md` (JSON das 81 capturas já existe); atualizar matriz com a validação real das correções acima.
- Pesquisa oficial de novidades e de moreformats já foi feita antes da pausa, HTML em `.codex/bc5-web`. Ainda falta relatório conciso próprio de formatos; não importar plugins em massa. O plugin CSV como Texto conflita com abertura automática de tabelas, e Excel Extended só primeira planilha não substitui o loader multi-planilha.
- Continuam pendentes a auditoria completa das telas/menus e os módulos ausentes. Word Wrap está explicitamente pendente em `BC5-PARIDADE.md`.
- Craft 0.3.0 funciona para capturas e leitura UIA. Pressionar menus principais é observável; submenus falharam `element_not_found` e não foram todos inspecionados. Computer Use indisponível por falha de kernel assets, já informada. Não alegar auditoria visual completa.

## Referências do último pedido

### Pendência acrescentada durante a pausa: botão direito na célula

Conferir e implementar o menu contextual de Comparar Tabelas, além do duplo
clique. Evidência fornecida pelo usuário:
`C:/Users/Altieres/AppData/Local/Temp/codex-clipboard-21bddcd2-dae6-44f2-a070-f864ad635211.png`.
**CONFIRMADO NA IMAGEM** (comportamento dos comandos ainda não inspecionado):

| Comando | Atalho visível |
|---|---|
| Alinhar Com (W)... | F7 |
| Copiar para Direita (Y) | Ctrl+R |
| Copiar células para a direita (Y) | Ctrl+Shift+R |
| Recortar | Ctrl+X |
| Copiar | Ctrl+C |
| Colar (P) | Ctrl+V |
| Excluir (D) | — |
| Excluir Linha (D) | — |
| Excluir Coluna (D) | — |
| Inserir Linha Antes (B) | Ctrl+Shift+Enter |
| Inserir Linha Após | Ctrl+Enter |
| Inserir Coluna Antes | — |
| Inserir Coluna Após | — |
| Modificar... | F2 |
| Classificar... | — |
| Abrir Arquivo... | Ctrl+O |
| Abrir Área de Transferência | Ctrl+Shift+V |
| Abrir Com (W) | Submenu |
| Salvar Arquivo | Ctrl+S |
| Salvar Arquivo Como... | F12 |
| Explorer | Submenu |

Na imagem, Colar, Classificar e Salvar Arquivo aparecem desabilitados; há
divisórias após a cópia entre lados, antes de Abrir Arquivo e antes de Explorer.
Não fixar esses estados: conferir dependências de seleção, edição, clipboard,
alterações não salvas e formato somente leitura. Conferir também o menu do lado
direito (direção/atalhos), sem presumir equivalência. Inspecionar o conteúdo dos
submenus; a imagem não o revela. Validar efeito real das ações em fixtures,
preservação dos dados/codificação, seleção/foco e confirmação/cancelamento.
Não ativar comandos sem implementação. **Apenas documentação foi alterada após
este pedido; a implementação permanece pausada.**

Original Home: `C:/Users/Altieres/AppData/Local/Temp/codex-clipboard-0ae541e4-0650-49a9-8d24-1913c753f53c.png`; AnyCompare anterior: `codex-clipboard-e68dd474-1090-49c6-a971-48b4e547c924.png` no mesmo diretório. Referência isolada Home: `.codex/bc5-ui-audit/home-new-probe.png` e árvore JSON; original mostra 12 atalhos, sidebar aproximadamente 20%, primeira faixa 64 px e segunda 32 px.
