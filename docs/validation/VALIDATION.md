# Validação da versão 0.1.2 — 14/10/2026

> Histórico. A entrega atual está em [0.1.12/VALIDATION.md](0.1.12/VALIDATION.md).

Executável: `C:\Projetos\BC4\anycompare\dist\AnyCompare-0.1.2.exe`, 7.196.160 bytes.
`dist/AnyCompare.exe` contém a mesma versão.

SHA-256: `8e48e3f8639d5200c507fb64aaebb1273e0ee963739476ec9eb81286bf7b15e1`.

- `cargo test --locked`: 20 testes passaram; um benchmark manual ignorado na execução padrão.
- `cargo clippy --all-targets --locked -- -D warnings`: passou sem avisos.
- `cargo build --release --locked`: compilação otimizada concluída.
- Testes enviam eventos reais do widget egui: digitar Unicode, colar, desfazer e usar setas com filtro.
  Também verificam que salvar não inclui linhas de alinhamento e que tornar os textos iguais recalcula as seções.
- Cliques nas faixas +/− expandem/recolhem os dois lados; editar as linhas reveladas altera o documento real.
- Métricas de cursor, Unicode e tabs mantêm as mesmas posições ao entrar/sair do viewport.
- Atualização localizada e comparação completa coincidem nos casos testados de substituição, inserção,
  remoção, linhas vazias, Unicode e limites do arquivo.

| Tela | Processo encerrou | Código de saída | stderr | Captura |
| --- | --- | ---: | ---: | --- |
| Home | Sim | 0 | vazio | [home.png](home.png) |
| Texto com fixtures | Sim | 0 | vazio | [text.png](text.png) |
| Pastas com arquivos reais | Sim | 0 | vazio | [folder.png](folder.png) |
| Hexadecimal com arquivos ICO distintos | Sim | 0 | vazio | [hex.png](hex.png) |
| Editar ativado, resumos do usuário | Sim | 0 | vazio | [text-editing.png](text-editing.png) |
| Editar desligado, resumos do usuário | Sim | 0 | vazio | [text-read-only.png](text-read-only.png) |
| Filtro Diferenças, Editar ativado | Sim | 0 | vazio | [text-differences.png](text-differences.png) |

A Home também foi iniciada a partir da pasta vazia associada ao chat, sem a pasta assets,
para verificar que os recursos visuais necessários estão incorporados no executável.
As capturas foram produzidas pelo renderizador da aplicação. Não constituem validação de cada ação por entrada de mouse/teclado.
As três capturas com os resumos usam janela interna de 960×1000. Seus hashes de entrada foram
conferidos antes/depois e permanecem idênticos, registrados no JSON. O status mostra 1.263 seções diferentes.
Para repetir as capturas, execute `tools/validate_release.ps1`.

O REA voltou a inspecionar o executável entregue. Os sete payloads do grupo ICO incorporado
coincidem exatamente com os sete payloads MAINICON da referência autenticada.
Evidência final: `ev_05c47a358bd3eec13b7877aecec88a0fbc906f0784534e11e5ed756a918f9300`.

## Digitação

Replay de 12 eventos `egui::Event::Text`, com os mesmos dois resumos, filtro Diferenças,
janela interna 960×700 e build release. Os arquivos são apenas lidos; a edição fica na memória do teste.
Mede entrada/layout egui, sem envio à GPU nem latência do Windows.

| Versão | Média por tecla | Máximo |
| --- | ---: | ---: |
| 0.1.1, antes da correção | 313,77 ms | 354,08 ms |
| 0.1.2 | 26,54 ms | 33,07 ms |

A média caiu aproximadamente 92%, cerca de 12 vezes. Não representa todas as operações/tamanhos de arquivo.

- [Log anterior](performance-before.log)
- [Log final](performance-after.log)
- Repetir: `cargo test --release --locked profile_edit_latency -- --ignored --nocapture`.

- [Resultado da inicialização das quatro telas](release-smoke.json)
- [Evidência REA do executável entregue](rea-release-resources.json)
- [Escopo e limitações da reconstrução](../STATUS.md)
