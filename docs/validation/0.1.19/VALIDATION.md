# Validação 0.1.19

- Executáveis: `dist/AnyCompare-0.1.19.exe` e `dist/AnyCompare.exe`, ambos com 21.510.144 bytes.
- SHA-256: `99139bd0d0ec2319c8b77d4510634254934c198da73e93758f7cca4e8fdeb11c`.
- `cargo fmt --check`: aprovado.
- `cargo test --locked`: 114 aprovados, três manuais ignorados.
- Teste manual `profile_user_tables`: aprovado nos dois pares Global.csv/Missions.csv, 12 colunas, sem diferenças e preservando todas as ocorrências de chaves.
- `cargo clippy --all-targets --locked -- -D warnings`: aprovado.
- `cargo build --release --locked`: aprovado.
- `tools/validate_release.ps1`: 65 capturas, todas com saída zero e stderr vazio. As verificações já existentes de divisórias/grade de Pastas/popup Sessões também passaram.
- Arquivos resumo do Desktop preservados por SHA-256 antes/depois, conforme release-smoke.json.
- Hashes dos quatro CSVs de tradução conferidos antes/depois das capturas adicionais, sem alterações. Relatório em `.codex/table-user-validation/integrity.json`; não incluir traduções do usuário no repositório.

As capturas de Tabelas comprovam inicialização/desenho dos formatos e menus exercitados. Os testes verificam leitura de valores, fórmulas, seleção de planilhas, chave duplicada, registros multilinha, células vazias, edição/cópia/desfazer, integridade ao salvar e seleção sem salto. Não comprovam equivalência total com BC4 nem gravação de Excel/ODS, que permanece indisponível.

Ver `../../STATUS-TABELAS-0.1.19.md` para limites, escolhas de cabeçalho e continuidade.
