# Replay invertido do original

Entradas: cópias somente de leitura de `resumo 1.txt` (esquerda) e `resumo.txt`
(direita). `inputs.json` registra os hashes; o executável portátil próprio tem
o mesmo SHA-256 da referência. Nenhum arquivo de usuário é saída do script.

`report.html` e `report.xml` foram produzidos pelo BC4 original. `expected.json`
preserva os textos, números das linhas, estados e cores desses relatórios.
O número 1.263 é derivado dos blocos contíguos com estado diferente de `same`
no XML, pelo mesmo critério do teste do relatório direto.

O hook do getter de contagem **não foi chamado** durante este relatório de linha
de comando; não é a origem dessa contagem. A evidência Frida registra o processo
controlado e, na segunda execução, os seis valores da tabela de resultados.
Não afirmar observação de um contador de interface com base nesse hook.

`rea-runtime.json` / `rea-labels-runtime.json`: observações REA/Frida 6.5.0.
O bundle está em `docs/rea/evidence-bundle-r7-runtime-rea650-2026-10-25.json`.
Não importar esse bundle na 6.7.0 (incompatibilidade de integridade documentada).

`tools/prepare_reverse_reference.py` prepara cópias e script; a execução do alvo
deve continuar isolada. O marcador `BCState.xml` é recriado antes de cada replay.
