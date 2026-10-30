# AnyCompare — instruções para continuidade

> **Entrega atual 0.1.17.** Continuar por `docs/STATUS-ABAS-0.1.17.md`. A prioridade é completar Comparar Pastas do BC4. Os registros anteriores são históricos.

> **Prioridade explícita de 30/10/2026:** concluir o Beyond Compare **4 100%**
> antes de adicionar novidades do Beyond Compare 5. Não iniciar implementações
> da versão 5 enquanto houver pendências da 4. Leia `docs/ESCOPO-BC4.md`.


> **30/10/2026 — popup:** versão atual **0.1.14**. Leia
> `docs/STATUS-POPUP-2026-10-30.md` e `docs/validation/0.1.14/VALIDATION.md`.
> Popup medido no original: fundo branco, 196×20/196×218 px, limite 30 linhas
> confirmado por REA/Frida. 23 rotinas adicionais decompiladas. Novo cálculo de
> dimensões, grupo vazio removido e entrada de edição bloqueada atrás do popup.
> Cores/sombra/ClearType e sessões completas ainda não equivalentes.
> As notas sobre 0.1.13 e anteriores abaixo são históricas.

> **30/10/2026 — Sessões:** versão atual **0.1.13**. Leia
> `docs/STATUS-SESSOES-2026-10-30.md` e `docs/validation/0.1.13/VALIDATION.md`.
> Sessões abre a árvore própria nos três comparadores; não restaurar o atalho
> falso à Home. 80 testes e 33 capturas. Foi corrigida a codificação do script
> de validação; capturas antigas com rótulos de menu corrompidos não provam popup.
> Quatro handlers de Sessões decompilados e exportados no bundle r10. O evento
> genérico de ativação e a geometria do popup continuam desconhecidos.
> A auditoria de 30 casos Hex da 0.1.12 permanece passando. Notas abaixo históricas.

> **27/10/2026: leia `docs/PROXIMA-IA.md` e `docs/STATUS-2026-10-27.md`.**
> A versão atual é a **0.1.11**. Foram corrigidas bordas/colchetes do Texto,
> seleção/arrasto sem salto no Hex e o alinhamento de bytes inseridos/excluídos.
> Há 75 testes e 26 capturas. Fontes/campos/menus e a equivalência binária completa
> continuam pendentes; não declarar a interface ou todos os comparadores concluídos.

## Pausa de 16/10/2026 (noite): ler primeiro

Leia **docs/RETOMADA-2026-10-17.md**. Ela traz:

- a entrega 0.1.6 em `dist\AnyCompare.exe` (SHA-256 `9bb7d069…12d42fb`, 22 telas validadas);
- a regra de composição dos menus confirmada estaticamente pelo REA/Ghidra
  (`docs/rea/MENUS-REGRA-COMPOSICAO.md`);
- as lacunas REA abertas e como retomar a sessão REA;
- a decisão do usuário de seguir a **edição Pro**;
- a próxima ordem de trabalho.

A versão fonte é **0.1.6**. Nada foi commitado. As seções abaixo descrevem estados anteriores.

## Estado atual prioritário — 16/10/2026

Leia **docs/STATUS-2026-10-16.md** antes das notas históricas abaixo. Versão
fonte 0.1.5; Ghidra concluiu a análise e há pseudocódigo/evidências em
docs/reference/ghidra-session. Os relatos antigos de ausência do Ghidra e o
teste de cores falhando foram superados. O teste completo dos resumos passou:
5.954 linhas, zero divergências de cores/caracteres e de pares alinhados,
1.263 seções; isso não prova todos os formatos/regras do original.

Nitidez: patch local de epaint em vendor/epaint, documentado em
vendor/epaint/ANYCOMPARE-PATCH.md. Segoe UI usa métricas/glifos hintados pelo GDI,
validados em quatro DPIs; o atlas ainda não tem ClearType RGB. Ícones usam a
escala original e NEAREST. Não voltar à fonte/filtragem anterior sem comparar.

Recentes pedidos ativos: listas suspensas nos caminhos e na busca, seleção azul,
divisórias entre grupos, nitidez e fidelidade geral de menus/controles. Pesquisa
fica acima dos caminhos. Histórico próprio do AnyCompare, sem importar o perfil
original. Todos os módulos/501 comandos ainda não estão completos.

Codificação foi revista: 7 encodings, CRLF/LF/CR/misto, preservar bytes/EOL,
reabrir com encoding, rejeitar UTF-32 e Unicode inválido, conflito por aliases,
aviso ANSI com escolha de encoding. Logs finais e entrega em
docs/validation/0.1.5; o JSON release-smoke registra o hash exato verificado.

As instruções de paths/preservação abaixo continuam válidas. Se divergirem do
estado técnico descrito aqui, trate as seções antigas como histórico.

## Objetivo e caminhos

Reconstruir o Beyond Compare 4.4.7.28397 em Rust/eframe/egui, incluindo funcionamento,
interface e ícones. O objetivo completo ainda não foi atingido.

- Código: C:\Projetos\BC4\anycompare (use este workdir).
- Referência: C:\Projetos\BC4\bc4\BCompare.exe.
- A pasta C:\Users\Altieres\Documents\ChatGPT\anycompare contém somente o apontamento deste chat.
- Leia README.md e docs/STATUS.md antes de continuar; o briefing anterior não refletia o funcionamento real.

## Instrução explícita do usuário

Em 14/10/2026 o usuário pediu a remoção total da faixa vermelha da Home. Ele sempre fecha
essa parte no original. Não recriar banners de suporte, vídeos, registro ou texto promocional.
Use as capturas em docs/reference/screenshots como referência, juntamente com os formulários recuperados.

## REA e recursos

Use o REA para inspecionar os artefatos de referência quando pertinente. A inspeção de recursos PE
já foi realizada e está em docs/reference/rea-pe-resources.json, com hash do executável e de cada payload.
729 recursos; 200 formulários TPF0 completamente interpretados; 501 declarações de ações;
340 imagens por escala, 1.360 variantes no total, e 17 grupos ICO.

- tools/inspect_reference.py verifica integridade e reproduz extração/catálogo (Python + Pillow).
- docs/reference/forms: hierarquias completas em JSON; docs/reference/FORM_MAP.md: índice.
- assets/reference/icons/<escala>/<lista>: preserva grupos; HomeSmallImages não deve sobrescrever HomeImages.
- src/ui/icon_catalog.rs é gerado. Execute cargo fmt após regenerá-lo.
- assets/reference/ico/MAINICON.ico é usado por build.rs para o recurso Windows do exe.
- O REA não tinha Ghidra/IDA configurados. Não afirmar decompilação ou equivalência funcional com base nos recursos.
- BCompare.tr foi recusado pelo REA por entrada de recursos sem alinhamento; o erro está registrado.

## Código atual

- main.rs: sessões em abas, drag/drop, inicialização por argumentos, captura pelo renderizador.
- document.rs: UTF-8/BOM, UTF-16 LE/BE/BOM, Windows-1252, PC/Unix, salvamento e conflito externo.
- history.rs: histórico próprio em APPDATA\AnyCompare\sessions.json. Preservar arquivos inválidos/não suportados.
- diff/mod.rs: Myers, chunks de palavras/caracteres e cópia de blocos inseridos/excluídos/substituídos.
- ui/home.rs: Home sem faixa vermelha e sem sessões inventadas.
- ui/text_compare.rs: comparação, edição, filtros/contexto, cópia, desfazer, abrir/salvar e scroll sincronizado.
- ui/aligned_editor.rs: layout nativo de edição com cores e alturas para lacunas; jamais inserir caracteres fictícios no buffer.
- ui/text_view.rs: mapa de posições das linhas e faixas filtradas de 9 px; expansão sincronizada entre os lados.
- ui/folder_compare.rs: comparação recursiva em thread, conteúdo/tamanho/data, filtros e abertura de pares.
- ui/hex_compare.rs: comparação binária por posição, somente leitura.
- ui/chrome.rs e theme.rs: degradê, cores amostradas e tipografia da referência.
- ui/icons.rs: seleção de escala e cache de imagens incorporadas.

Ainda pendentes: Folder Sync, Folder/Text Merge de três vias, regras avançadas, árvore agregada de pastas,
cópias de arquivos/pastas, outros comparadores, perfis remotos/archives e relatórios.
Não transformar um módulo ausente em atalho para uma tela diferente, nem deixar ações aparentemente funcionais sem implementação.

## Compilação e verificação

Rust MSVC 1.99.0 e Windows SDK 10.0.26100.0 estão instalados. Neste ambiente o cargo pode exigir
C:\Users\Altieres\.cargo\bin\cargo.exe; não alterar o PATH global.

- cargo test --locked: 20 testes de regressão; mais um benchmark manual ignorado na execução padrão.
- cargo clippy --all-targets --locked -- -D warnings.
- cargo build --release --locked; entrega em dist\AnyCompare.exe.
- --text <left> <right>, --folder <left> <right>, --hex <left> <right>.
- --capture <arquivo.png>: captura pelo renderizador, encerra após salvar; não grava histórico de teste.
- --read-only, --differences e --size <largura> <altura>: modos reproduzíveis para capturas.
- tools/validate_release.ps1: sete inicializações com captura; os arquivos resumo do Desktop são somente lidos e seus hashes conferidos.

A partir de 0.1.1, Editar só muda a possibilidade de digitar; mantém as diferenças e o alinhamento.
Contar seções contíguas para a barra de status (os dois resumos do usuário resultaram em 1.263),
não confundir essa contagem com linhas diferentes. Interface OpenGL; comparação na CPU.
Ao abrir um par, reutilizar sessão vazia compatível ou ativar o mesmo par já aberto; preservar edições.
Backup anterior a esta correção: .codex/backups/before-aligned-edit-2026-10-14.zip e before-aligned-edit-0.1.0.exe.

Na versão 0.1.2, as faixas +/− são elementos do mapa visual; nunca devem virar caracteres no buffer.
O editor mantém métricas do documento completo, mas cria malhas de glifos somente perto do viewport.
O cache depende do conteúdo/cores/posições visíveis, DPI e opções; uma revisão global sozinha não invalida tudo.
DiffEngine::update recompõe somente o trecho alterado entre âncoras; mudanças grandes usam comparação completa.
Chunks imutáveis são compartilhados por Arc. Reaproveitar o resultado calculado pelo editor no commit da edição.
Benchmark manual: cargo test --release --locked profile_edit_latency -- --ignored --nocapture.
Logs antes/depois em docs/validation/performance-*.log: média 313,77 ms → 26,54 ms por tecla (sem envio à GPU).
Fonte de texto atual: Courier New regular, 12,4 px lógicos, linhas de 16 px; não tratar MRUFonts como fonte ativa comprovada.
Backup da 0.1.1: .codex/backups/before-folds-performance-2026-10-14.zip e before-folds-performance-0.1.1.exe.

O exe recebido originalmente tinha panic no layout por dimensão negativa, reproduzido e corrigido.
O backup anterior às alterações está em .codex\backups\before-rea-recovery-2026-10-14.zip.
Mantenha docs/STATUS.md atualizado com evidências, limitações e trabalho pendente.

## Continuação ativa: completar menus (14/10/2026)

O usuário pediu revisão de tudo e depois “veja agora todos os menus e complete”.
Não encerrar com um inventário apenas. A revisão está em tools/audit_reference.py e docs/review,
mas precisa ser atualizada após as implementações: os números da geração 0.1.3 ficaram antigos.
Não afirmar que todas as 501 declarações ou todos os 200 recursos já foram implementados.

0.1.3 foi compilada e entregue em dist/AnyCompare.exe e dist/AnyCompare-0.1.3.exe (7.167.488 bytes,
SHA e0e75d0c57beacf92c1eb2a1793c60296175af4c32793591a06d696f82f4f83d).
REA ev_4be1e7c7335aa8dc62b85455227f07e880e7aedc495b6032f57be32233fef869.
29 testes passaram; benchmark separado média 28,09 ms; oito capturas, exit 0/stderr vazio.
Não houve mensagem final dessa entrega; o usuário acrescentou a solicitação de completar menus.

O código em desenvolvimento é 0.1.4; não confundir com o exe 0.1.3 já compilado.
Já acrescentados no código: barra localizar/substituir Unicode/regex/case/palavra inteira,
pesquisa em ambos os lados, F3/ShiftF3/CtrlF/CtrlH; CtrlG com prompt nativo linha:coluna;
cópia de linha CtrlShiftR/L, seleção de seção CtrlD, inserir/excluir/recuar linhas e conversões;
opções nativas da tela de texto, formato encoding/EOL, regras básicas case/espaços.
32 testes passaram (mais benchmark manual ignorado) antes desta atualização de versão.
Faltam concluir os menus de pastas e hex, relatórios/mais comandos, ampliar testes e entregar exe.

Todos os diálogos existentes passam por src/ui/dialogs.rs (DialogHost), parentados ao eframe::Frame.
Salvar/erro/Sobre usam MessageBoxW através de rfd. native_prompt.rs cria DLGTEMPLATE alinhado
com controles Windows para entrada/check/combobox; mesma modalidade/botões/Enter/Escape.
Não reintroduzir egui::Window para confirmações/caixas modais. Os testes usam TestDialogs;
respostas mockadas não equivalem a teste de clique ou screenshot da caixa real do Windows.
Menus de pesquisa ficam em frame embutido (assim é TFILEFINDFRAME recuperado), não diálogo flutuante.
Regex 1.13.1, windows-sys 0.59 e raw-window-handle 0.6 foram adicionados e fixados em Cargo.lock.
Regras preservam buffers originais e line numbers; usar update_rules também no layouter.
Sem regras mantém diff incremental; com regras básicas usa comparação completa, validar desempenho.

## Pausa e ponto de retomada (14/10/2026)

O usuário pediu explicitamente para parar e continuar amanhã, usando REA com Ghidra
para resolver as inconsistências visuais. Não continuar durante a pausa nem agendar
execução sem pedido. Leia primeiro **docs/RETOMADA-2026-10-15.md** ao retomar:
esse registro corrige o estado antigo descrito acima, documenta o Ghidra instalado,
o timeout da primeira análise, a evidência nativa de cores, um novo teste ainda falhando,
as alterações ainda não compiladas e a revisão pendente de codificação.
