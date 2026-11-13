# BC5 Media Compare — Rust (v0.1.0)

Módulo de comparação de mídia **independente** para aplicações em Rust. Inspirado no comportamento documentado do **Media Compare** do Beyond Compare 5. Não incorpora código do fabricante nem afirma compatibilidade 1:1.

## Funcionalidades implementadas no código-fonte

- Sondagem pelo conteúdo com `lofty` (tags MP3/ID3v1/ID3v2, FLAC/Vorbis, MP4/ilst e outros formatos aceitos pela biblioteca).
- Normalização de nomes de tags e propriedades técnicas: duração, taxa de amostragem, bitrate, canais, profundidade e tamanho.
- Múltiplos valores por campo; dados binários representados por tamanho e BLAKE3 (não copiados para JSON).
- Arte incorporada: metadados, hash e exportação sem editar o original.
- Comparação semântica de tags, classificando diferenças importantes, sem importância ou iguais.
- Regras serializáveis: importância por campo, exclusões, comparação case-sensitive/insensitive, trim e supressão de diferenças secundárias.
- Sessão serializável com filtros Todos/Diferenças/Iguais/Nenhum, navegação próximo/anterior, swap, reload transacional, bloqueio e recálculo.
- Relatórios JSON, texto e HTML com escape de conteúdo e destaque lado a lado.
- CLI para comparação, inspeção, extração de capa e abertura via aplicativo padrão.
- Testes de unidade para regras, normalização, filtros, navegação, JSON e escape HTML.

## Build

Instale Rust e Cargo em seu computador, a seguir:

```bash
cargo build --release
cargo test
```

Exemplos:

```bash
cargo run -- inspect "artista1.mp3" --json
cargo run -- compare "esquerda.flac" "direita.flac" --format html --output relatorio.html
cargo run -- compare "a.m4a" "b.m4a" --ignore-unimportant --differences-only
cargo run -- cover "musica.mp3" --index 0 --output capa.jpg
cargo run -- play "musica.mp3"
```

## Integração GUI

Chame `inspect_media()` em tarefas de fundo para não bloquear a thread principal. Guarde os dois `MediaInfo` na `MediaSession`; `compare()` fornece linhas classificadas; `visible()` aplica o filtro; `next_difference()` implementa navegação; `set_importance()` recalcifica o campo. O UI pode usar um painel de duas colunas, detalhes e botões de mídia.

A reprodução usa `open::that()` apenas como *launch* do player associado. Para **Play Both sincronizado, seek, pausa e volume individuais**, integre um backend de áudio real na GUI; a CLI *não* implementa sincronização.

## Limitações e QA

Esta entrega é **código-fonte pronto para integração e teste, não um binário certificado**. O ambiente desta análise não possui `cargo`/`rustc`, portanto a compilação e os testes Rust **não foram executados**. Não há prova de equivalência byte a byte/funcional com o Beyond Compare, nem testes com arquivos FLAC/MP3/AAC reais fornecidos pelo usuário. A análise estática do `BCompare.exe` recuperou nomes de classes e comandos, mas não a semântica integral do código da fabricante.

Não altera arquivos de entrada. Os tipos com codecs/formatos adicionais suportados por Lofty são extensão voluntária à cobertura publicada para Media Compare 5 (FLAC, MP3 e MP4/AAC).

Fontes consultadas: documentação do fabricante: `https://www.scootersoftware.com/v5help/commandsmedia.html`, `https://www.scootersoftware.com/v5help/viewother.html`, `https://www.scootersoftware.com/v5help/scripting_reference.html`; API Lofty: `https://docs.rs/lofty/0.25.5/lofty/`.

### Nota da Etapa 7 (análise forense)

`RawMediaObservation.file_blake3` agora registra um hash integral do arquivo para impedir que uma mutação do conteúdo de áudio sem alteração dos trechos de tags passe na interpretação posterior. Snapshots legados sem o campo continuam legíveis, com aviso de verificação parcial. **Isso não replica a igualdade do BC5.** O motor comparador do usuário continua experimental enquanto não existirem relatórios originais do Windows.
