# AnyCompare

Reconstrução em Rust da interface e das funções do Beyond Compare 4.4.7.28397.
A cópia completa está em desenvolvimento; o inventário de recursos não comprova equivalência funcional.

Estado atual: [30/10/2026 — versão 0.1.17](docs/STATUS-ABAS-0.1.17.md).

## Executar

Compile com Rust MSVC no Windows usando `cargo build --release --locked` e abra
`target/release/anycompare.exe`. Os ícones necessários estão incluídos no código-fonte e
são incorporados ao executável. As entregas locais também ficam em `dist/AnyCompare.exe`.

```powershell
cargo run -- --text arquivo-esquerdo.txt arquivo-direito.txt
cargo run -- --folder pasta-esquerda pasta-direita
cargo run -- --hex binario-esquerdo binario-direito
```

Também é possível selecionar arquivos/pastas pelos botões ou arrastar um par para a janela.
As sessões abertas permanecem em abas abaixo do menu, com ícones, seleção e fechamento. O histórico fica em `%APPDATA%\AnyCompare\sessions.json`.

## Disponível nesta versão

- Árvore Sessões nos comparadores de Texto/Pastas/Hex, com Novo e histórico próprio.
- Página inicial com cores e recursos do original; faixa vermelha removida por solicitação do usuário.
- Comparação de texto com alinhamento, cores por caractere, filtros e contexto; validada contra os relatórios do original nos dois resumos fornecidos.
- Faixas de linhas filtradas com contagem e botões +/− sincronizados para expandir/recolher os trechos.
- Edição dentro da comparação, preservando destaques, alinhamento e scroll sincronizado.
- Cópia do bloco selecionado (`Ctrl+R`/`Ctrl+L`), desfazer (`Ctrl+Z`) e salvar/salvar como.
- UTF-8/BOM, UTF-16 LE/BE com/sem BOM, Windows-1252; CRLF/LF/CR/misto e reabertura explícita com codificação.
- Verificação de alterações externas antes de salvar e gravação por substituição de arquivo temporário.
- Comparação recursiva de pastas em segundo plano, por conteúdo ou tamanho/data; filtros e abertura de pares.
- Comparação hexadecimal com alinhamento de inserções/exclusões, seleção em bytes/caracteres,
  cópia, edição, desfazer e salvamento nos offsets físicos. Auditoria de 30 casos executados no original;
  buscas de alinhamento que atingem o limite de tempo são indicadas na tela.
- Pesquisa/substituição Unicode/regex, histórico de buscas e de caminhos, sessões/workspaces e relatórios.
- PT-BR e inglês, seleção automática pelo Windows ou manual; catálogos de idiomas extensíveis.
- Ícones sem filtragem linear e Segoe UI hintada pelo GDI no Windows; patch documentado em vendor/epaint.
- Ícone original da janela e do executável, além de ícones de interface em quatro escalas.

## Referência e auditoria

O REA inspecionou `C:\Projetos\BC4\bc4\BCompare.exe`, SHA-256
`aba1b98f848a01c3729cfee3e8818ab8e0d947276bc42c9a55c1475c9f8438db`.

Foram verificados e extraídos **729 recursos**, **200 formulários Delphi**,
**501 definições de ações**, **17 grupos ICO** e **1.360 variantes de imagens**
(340 entradas por escala de 100%, 125%, 150% e 200%).

- [Evidência original do REA](docs/reference/rea-pe-resources.json)
- [Inventário de interface e imagens](docs/reference/ui-inventory.json)
- [Mapa dos 200 formulários](docs/reference/FORM_MAP.md)
- [Galeria pesquisável de ícones](docs/reference/icon-gallery.html)
- [Situação da reconstrução](docs/STATUS.md)
- [Capturas das telas executadas](docs/validation/)

Para repetir a extração, instale Pillow no Python utilizado e execute:

```powershell
python tools/inspect_reference.py
cargo fmt --all
```

O extrator verifica o hash do executável e de cada payload antes de recuperar os dados.
Preserva grupos de imagens separados; `HomeSmallImages` não sobrescreve `HomeImages`.

## Validação

```powershell
python tools/restore_fixture_directories.py
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo build --release --locked
```

Os testes de regressão cobrem inicialização/layout, codificações, mesclagem, proteção contra
alterações externas, comparação de pastas, leituras parciais, histórico, ícones e edição alinhada.
Os testes do editor enviam eventos de digitação, colagem, desfazer e navegação por linhas filtradas;
verificam Unicode e salvamento sem inserir caracteres de alinhamento no documento.
Cobrem também cliques de expansão/recolhimento, métricas de cursor fora da tela e equivalência
da atualização localizada com a comparação completa em inserções, remoções e substituições.
O teste manual `profile_edit_latency`, executado com `--release -- --ignored --nocapture`,
reproduz 12 teclas nos resumos do usuário sem salvar os arquivos.
O recurso de captura usa o próprio renderizador da aplicação e encerra o processo ao concluir:

```powershell
dist/AnyCompare.exe --capture docs/validation/home.png
dist/AnyCompare.exe --text tests/fixtures/left/example.txt tests/fixtures/right/example.txt --capture docs/validation/text.png
```

`--read-only` inicia o texto com edição desligada; `--differences` seleciona o filtro de diferenças.
`--size 960 1000` define as dimensões internas para comparar capturas. `tools/validate_release.ps1`
repete a inicialização das telas e registra capturas, códigos de saída e hashes dos arquivos de entrada.

A interface usa o renderizador OpenGL do eframe; a comparação de arquivos é calculada na CPU.
Monitores de FPS podem reconhecer a janela gráfica.

## Limites atuais

Sincronização de pastas, mesclagem de três vias, comparadores de imagens/dados/MP3/registro,
perfis remotos, regras avançadas e outros módulos/menus do original continuam pendentes.
Os controles ainda não implementados permanecem desabilitados.
A comparação de pastas usa uma lista de caminhos recursiva; a árvore hierárquica completa não foi reproduzida.
Não houve validação de equivalência binária nem verificação de cada ação do original em execução.
