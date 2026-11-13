# Comparador de Tabelas — 0.1.19, 02/11/2026

A tela de Tabelas foi implementada. CSV deixa de abrir automaticamente no comparador de texto. A abertura por argumentos, dois arquivos arrastados, arquivos arrastados separadamente e pares abertos por Pastas identifica os formatos tabulares. `--text` e `--hex` continuam escolhas explícitas. A Home, a árvore Sessões, os menus, as abas e os arquivos de sessão/workspace reconhecem a nova sessão.

## CONFIRMADO EM EXECUÇÃO

- Leitura de nove extensões com arquivos reais de teste: CSV, TSV, TAB, SSV (ponto e vírgula), PSV (barra vertical), XLSX, XLS, XLSB e ODS. O leitor também admite os contêineres Excel XLSM, XLAM e XLA; esses três não têm fixtures específicas nesta entrega. Macros não são executadas.
- Registros e células, aspas escapadas, separadores dentro de aspas, conteúdo multilinha, células vazias, larguras diferentes e ocorrências repetidas da mesma chave. As duplicatas são alinhadas em ordem de ocorrência, sem sobrescrever registros.
- Grade virtualizada com divisórias, colunas, números de linha física, cores por célula, seleção, rolagem vertical sincronizada, rolagem horizontal e seletor de planilha independente de cada lado.
- Filtros Todos/Diferenças/Iguais, navegação por diferenças e teclado, busca de valores inclusive em colunas fora da tela, inversão dos lados, recarga e regras configuráveis. O padrão desta entrega usa a primeira linha como cabeçalho e a primeira coluna como chave. Em Regras é possível desativar o cabeçalho, alinhar por outra coluna ou por posição e ignorar caixa/espaços das extremidades.
- Duplo clique/F2 abre o editor de célula dos arquivos delimitados. Aplicar integra a alteração à comparação; Cancelar preserva o valor anterior. Salvar/Ctrl+S, Salvar como, cópia de registros, desfazer/refazer e confirmação de alterações usam documentos reais. O ícone Salvar só fica ativo para o lado alterado.
- Os arquivos delimitados usam `Document`: preservação de codificação/BOM/terminadores e registros não editados, rejeição de perda ANSI, conflito externo e proteção contra salvar sobre o arquivo do outro lado.
- Nos arquivos Excel/ODS, o seletor carrega a planilha escolhida. Comparam-se valores armazenados, ou as fórmulas recuperadas quando essa opção é marcada. Não há recálculo.

Os testes detectaram perda de quebras de linha/tabulações em ODS no leitor Calamine 0.36.1. A dependência foi atualizada para 0.38.0, que preserva esses elementos. As fixtures verificam ambos os caracteres.

A primeira captura expôs sobreposição da barra, caminhos e metadados. Menu e toolbar agora reservam painéis próprios, como os demais comparadores; os dois lados dos metadados têm largura e recorte fixos. Um teste verifica que os caminhos estão abaixo da toolbar e que clicar numa célula não altera a rolagem.

## Validação

- `cargo test --locked`: **114 aprovados, 3 manuais ignorados**.
- `cargo test --locked profile_user_tables -- --ignored --nocapture`: aprovado. Global.csv: 1.010 registros, 12 colunas; Missions.csv: 537 registros, 12 colunas. Com cabeçalho excluído, 1.009 e 536 pares, respectivamente, sem diferenças nos dois pares fornecidos. Leitura/alinhamento no teste debug: aproximadamente 57 e 82 ms, sem incluir desenho.
- `cargo clippy --all-targets --locked -- -D warnings`, formatação e compilação release aprovados.
- Capturas e integridade em `validation/0.1.19/release-smoke.json`. As capturas de tabelas incluem os nove formatos, detecção automática, drop, busca, filtro e menus/Sessões em português e inglês.
- Hashes de Global.csv e Missions.csv conferidos antes/depois das capturas; nenhum arquivo de entrada foi modificado. Evidência local em `.codex/table-user-validation/integrity.json`; capturas que contêm traduções do usuário permanecem somente ali.
- Fixtures sintéticas reproduzíveis por `tools/create_table_fixtures.py`. Os quatro arquivos `any_sheets` vieram dos testes MIT do Calamine; commit, URLs e hashes estão em `tests/fixtures/tables/upstream.json`, com licença incluída.

## Limitações e próximo trabalho

Excel/ODS estão **somente leitura**. Não implementar gravação que perca estilos, fórmulas, macros ou outras planilhas. A edição implementada é dos arquivos delimitados. Salvar como preserva o delimitador configurado; não é conversão entre contêineres de planilha.

Ainda faltam equivalência visual e funcional completa com BC4, redimensionamento/ordenação interativa de colunas, seleção de intervalos, edição direta na grade, regras numéricas avançadas, relatórios de tabelas, outros formatos tabulares e maior confronto com o original. A linha de detalhes mostra a célula selecionada, não a grade completa de detalhes do BC4. Não há comprovação 1:1 para o módulo inteiro.

Os 18 relatórios nativos anteriores continuam em `review/craft-2026-11-02/tables`. Uma execução adicional isolada de `data-report` para Missions.csv confirmou que registros vazios são dados presentes no arquivo; o relatório fica somente em `.codex/table-user-validation`. O padrão de cabeçalho/perfis da referência não foi reproduzido integralmente. Não esconder registros vazios apenas para melhorar a captura.

REA 6.7.0 foi consultado: sem alvo aberto, Ghidra indisponível por configuração do JDK no processo do servidor. Não foi produzida nova decompilação nesta etapa. Os formulários e os relatórios nativos já recuperados foram usados como referência, sem alegar que o novo código veio de decompilação completa.

As demais pendências de `ESCOPO-BC4.md` continuam válidas. Os 729 recursos incorporados não equivalem a 729 funções implementadas. BC5 permanece fora desta etapa.
