# AnyCompare

Comparador e mesclador visual de arquivos e pastas escrito em Rust (`eframe`/`egui`), projetado com foco em desempenho nativo, alta fidelidade visual e operações seguras em disco.

![Build](https://github.com/altiereslima/AnyCompare/actions/workflows/build.yml/badge.svg)

## Funcionalidades

### Sessões de Comparação, Mesclagem e Edição
- **Comparar Pastas (`Folder Compare`)**: comparação recursiva por tamanho, data/hora e conteúdo (CRC/binário/regras), suporte a arquivos compactados (`.zip`, `.tar.gz`), filtros por nome e status, cópia atômica, exclusão para a Lixeira, atribuição de timestamp/atributos DOS e ações em lote.
- **Sincronizar Pastas (`Folder Sync`)**: modos *Atualizar Esquerda*, *Atualizar Direita*, *Atualizar Ambos*, *Espelhar Esquerda* e *Espelhar Direita*, com pré-visualização, proteção contra sobreposição de raízes e exclusão ordenada de diretórios vazios.
- **Mesclar Pastas (`Folder Merge`)**: mesclagem de 2 ou 3 vias (Esquerda, Centro/Base e Direita) para uma pasta de saída (ou *in-place*), com resolução de conflitos, proteção contra caminhos aninhados e execução segura em 2 passadas.
- **Comparar Textos (`Text Compare`)**: alinhamento linha a linha e destaque inline de diferenças, edição completa nos dois lados, copiar seções/linhas, desfazer/refazer, busca e substituição (incluindo regex), quebra de linha, painel de detalhes de caracteres, barra de visão geral, abrir da Área de Transferência (`Shift+Ctrl+V`) e conversão automática de documentos (**PDF, DOCX, ODT, PPTX, ODP, RTF, EPUB**).
- **Mesclar Textos (`Text Merge`)**: mesclagem interativa de 2 ou 3 vias com painel de saída editável em tempo real e navegação entre conflitos.
- **Editor de Texto (`Text Edit`)**: editor independente com suporte a múltiplas codificações (UTF-8, UTF-8 BOM, UTF-16 LE/BE, Windows-1252), preservação de quebras de linha (PC/Unix/Mac/Misto), marcadores e busca/substituição.
- **Comparar Tabelas (`Table Compare`)**: suporte a **CSV, TSV, XLSX, XLS, ODS e tabelas HTML**, comparação tipada (texto, número com tolerância, moeda, data/hora, booleano), ordenação por colunas-chave e edição de células.
- **Comparar Imagens (`Picture Compare`)**: modos *Tolerância*, *Intervalo* e *Misturar*, zoom sincronizado, deslocamento manual/automático (`ΔX`/`ΔY`) e estatísticas de pixels.
- **Comparar Hexadecimal (`Hex Compare`)**: comparação binária byte a byte com alinhamento, painel ASCII, inspeção Little/Big-Endian e edição hexadecimal.
- **Comparar Versões (`Version Compare`)**: inspeção e comparação de metadados PE (`EXE`/`DLL`), versões de arquivo/produto e recursos.
- **Comparar Registros (`Registry Compare`)**: visualização em 3 colunas (`Nome | Tipo | Dados`) de arquivos `.reg`, navegação por subchaves e edição completa com desfazer/refazer.
- **Comparar Mídia (`Media Compare`)**: comparação de metadados e tamanho de payload de áudio/vídeo (**MP3, WAV, FLAC, OGG/OPUS, M4A, MP4**).

### Recursos Gerais
- **15 idiomas integrados**: Português (Brasil), Inglês, Alemão, Espanhol, Francês, Irlandês, Italiano, Japonês, Coreano, Russo, Esloveno, Sueco, Tailandês, Vietnamita e Chinês Simplificado.
- **Abas de sessão e espaços de trabalho**: múltiplas sessões simultâneas na mesma janela, histórico de sessões recentes e exportação de relatórios.
- **Integração com o Windows Explorer**: extensão nativa de menu de contexto para Windows 10 (clássico) e Windows 11 (MSIX `IExplorerCommand`).

## Uso pela Linha de Comando (CLI)

```powershell
anycompare --folder pasta-esquerda pasta-direita
anycompare --sync pasta-esquerda pasta-direita
anycompare --folder-merge pasta-esquerda pasta-centro pasta-direita
anycompare --text arquivo-esquerdo.txt arquivo-direito.txt
anycompare --merge arquivo-esquerdo.txt arquivo-centro.txt arquivo-direito.txt
anycompare --table planilha-a.xlsx planilha-b.csv
anycompare --picture imagem-a.png imagem-b.png
anycompare --hex binario-a.bin binario-b.bin
anycompare --version app-v1.exe app-v2.exe
anycompare --registry reg-a.reg reg-b.reg
anycompare --media audio-a.mp4 audio-b.mp4
anycompare --edit arquivo.txt
```

Você também pode passar dois arquivos ou duas pastas diretamente (`anycompare arquivo1 arquivo2`) para detecção automática do tipo de sessão.

## Compilar a partir do Código-Fonte

### Pré-requisitos
- **Rust** (stable, edition 2021)
- **Windows**: Visual Studio Build Tools (MSVC) + Windows 10/11 SDK
- **macOS**: Xcode Command Line Tools (`xcode-select --install`)
- **Linux (Ubuntu/Debian)**:
  ```bash
  sudo apt-get update
  sudo apt-get install -y libgtk-3-dev libxkbcommon-dev libxcb-shape0-dev \
    libxcb-xfixes0-dev libwayland-dev libasound2-dev pkg-config libssl-dev
  ```

### Compilação e Testes

```bash
# Compilar em modo release
cargo build --release --locked -p anycompare

# Executar todos os testes
cargo test --locked -p anycompare

# No Windows, compilar também a extensão do Explorer:
cargo build --release --locked -p anycompare -p anycompare-shellex
```

### Gerar Instalador do Windows (Inno Setup + MSIX)

```powershell
# Requer Inno Setup 6 e Windows SDK (MakeAppx)
.\build.ps1
```

O instalador final é gerado em `dist/AnyCompare-Setup-<versao>.exe`.

## Estrutura do Projeto

- `src/` — núcleo do AnyCompare (motores de diff/merge, conversores de formatos e interface `egui`)
- `shellex/` — extensão de menu de contexto do Windows Explorer (`AnyCompareShellEx64.dll`)
- `installer/` — script Inno Setup, manifesto MSIX e registro do menu de contexto
- `assets/` — ícones e catálogos de tradução dos 15 idiomas
- `vendor/` — crates locais (`anycompare-media`, `anycompare-registry`, patches `egui`/`epaint`)
- `tests/` — fixtures utilizados pelo `cargo test`
- `tools/` — script de empacotamento do instalador Windows
