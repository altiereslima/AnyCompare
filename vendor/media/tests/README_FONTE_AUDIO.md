# Corpus de mídia sintético (não contém músicas de terceiros)

Os seis arquivos A/B foram criados por FFmpeg usando `sine=frequency=440:sample_rate=44100:duration=0.28`, canal mono e tags idênticas, exceto `title`: **Teste A** ou **Teste B**. Os formatos testados são **MP3, FLAC e M4A (AAC no contêiner MP4)**.

Comandos exemplares para recriar (pode haver diferenças nos bytes e na tag do codificador conforme a versão do FFmpeg):

```bash
for ext in mp3 flac m4a; do
  ffmpeg -hide_banner -loglevel error -y -f lavfi -i \
    'sine=frequency=440:sample_rate=44100:duration=0.28' \
    -ac 1 -metadata title='Teste A' -metadata artist='Comparador Rust' \
    -metadata album='Suite de Testes' "A.$ext"
done
```

São **fixtures criados**, não amostras extraídas do programa proprietário. A verificação via Python Mutagen comprovou o título, álbum e artista dos seis arquivos. Os testes Rust presentes no crate dependem de `cargo test` no computador do usuário; a execução deles não foi possível neste ambiente, pois não há `cargo`/`rustc` instalados.

`A_with_cover.mp3` contém uma imagem JPEG azul 32×32 gerada sinteticamente por FFmpeg para o teste de extração de capa, sem imagens de terceiros. O extrator usa criação exclusiva (`create_new`), recusando substituir destinos existentes, inclusive o arquivo de áudio original.
