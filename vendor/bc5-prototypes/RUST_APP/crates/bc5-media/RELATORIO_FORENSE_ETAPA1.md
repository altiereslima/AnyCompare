# BC5 Media Compare — primeira etapa de engenharia reversa

Data: 2026-11-03. Alvo: `BCompare-5.2.7.32913.exe` fornecido pelo usuário.

## 1. Extração do instalador

- Formato identificado: **Inno Setup 6.6.1**.
- Foram encontrados **22 fluxos de arquivos** com assinatura `zlb\x1a` e LZMA2. Todos os 22 decodificaram até o término do fluxo.
- O cabeçalho Inno Setup foi recuperado em **2 blocos LZMA1**. As somas CRC32 dos subblocos de 4096 bytes foram conferidas.
- O cabeçalho fornece os nomes originais dos 22 arquivos e as condições de instalação, permitindo montar as árvores `Windows_x86` e `Windows_x64` sem conflito entre arquivos homônimos.
- O instalador **não foi executado**. Nenhuma configuração de registro, serviço, associação ou licença foi alterada.
- Todos os arquivos extraídos têm tamanho, deslocamento da origem e SHA-256 registrados no `MANIFESTO_SHA256.json`.

## 2. Arquivos recuperados

| Entrada | Arquivo original | Variante | Tamanho (bytes) |
|---:|---|---|---:|
| 0 | 7z.dll | x86 | 1.331.680 |
| 1 | BCompare.exe | x86 | 37.253.152 |
| 2 | mscoree.dll | x86 | 12.008 |
| 3 | PdfToText.exe | x86 | 1.163.920 |
| 4 | BCUnRAR.dll | x86 | 339.664 |
| 5 | 7z.dll | x64 | 1.920.992 |
| 6 | BCompare.exe | x64 | 50.566.184 |
| 7 | BCShellEx.msix | x64 | 359.381 |
| 8 | BCShellEx64.dll | x64 | 786.128 |
| 9 | mscoree.dll | x64 | 12.008 |
| 10 | PdfToText.exe | x64 | 1.441.424 |
| 11 | BCUnRAR.dll | x64 | 412.368 |
| 12 | BCClipboard.exe | Comum | 1.356.288 |
| 13 | BCDarkTheme.vsf | Comum | 117.919 |
| 14 | BComp.com | Comum | 125.592 |
| 15 | BComp.exe | Comum | 119.448 |
| 16 | BCompare.chm | Comum | 2.704.482 |
| 17 | BCShellEx.dll | Comum | 540.368 |
| 18 | Is64Bit.dll | Temporário do instalador | 30.720 |
| 19 | License.html | Comum | 8.426 |
| 20 | Patch.exe | Comum | 123.160 |
| 21 | Readme.txt | Comum | 1.441 |

## 3. Craft Anything no executável real

**Versão da ferramenta:** Craft Anything 0.3.0, mecanismo nativo `builtin`, modo Deep.

**Alvo examinado:** `Windows_x64/BCompare.exe` (PE32+, x86-64, código nativo, 50.566.184 bytes).

**SHA-256 do EXE x64:** `c8f68dc33424c02a50f432ce7327b14fffd78ea3b1ca4b14ac52e4c275eeb086`.

O analisador relatou 95.780 strings, 1.142 imports, 2.017.274 instruções e **50.000 funções identificadas**. A contagem exata de 50.000 pode refletir um limite do mecanismo, portanto **não representa um total comprovado de funções do aplicativo**.

A busca de `Media` encontrou **103 ocorrências**. Entre os identificadores recuperados em `.rsrc` estão:

- `TMEDIAACTIONS`, `TMediaActions`, `TMediaView`, `TMediaReportDialog`.
- `MediaRules`, `MediaMap`, `MediaSession`, `MediaContent`, `DlgMediaReport`, `ViewMedia`.
- `OpenMediaCompare`, `CompareUsingMediaCompare`, `MediaCompareNew`.
- `imgMediaPlay`, `imgMediaPause`, `imgMediaFormat`.
- `Media Compare Report`, `Media Format`, `Lyrics`, `Media type`, `Mix artist`.

**Limite importante:** esses nomes foram observados nos recursos do executável. A busca `core-feature-find` pelo texto `Media Compare` encontrou duas strings, mas **nenhuma referência a função**. Portanto, isso não é descompilação integral do fluxo de comparações, nem prova da implementação interna.

## 4. Alvo Rust

Uma primeira implementação independente (`BC5_MEDIA_RUST`) inclui:

- leitor de metadados do Lofty, normalização de tags, propriedades técnicas e capas;
- motor de comparação de campos com regras configuráveis de importância;
- sessão de comparação, filtros, troca de lados, recarga, navegação;
- relatórios texto, JSON e HTML;
- CLI de inspeção, comparação, extração de capas e abertura em player padrão;
- testes unitários para regras, filtros, navegação, serialização e segurança HTML.

**Não validado ainda:** compilação/testes Rust reais, integração da GUI, decodificação/reprodução sincronizada nativa, comparação de resultados contra arquivos de referência no BC5, equivalência exata com regras da fabricante. Não foi utilizada fonte proprietária no código Rust.

## 5. Referências externas

- https://www.scootersoftware.com/v5help/viewother.html
- https://www.scootersoftware.com/v5help/commandsmedia.html
- https://www.scootersoftware.com/v5help/scripting_reference.html
- https://docs.rs/lofty/0.25.5/lofty/
