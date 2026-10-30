# Patch do AnyCompare sobre epaint 0.29.1

Fonte: crate epaint 0.29.1 instalado no cache Cargo, licenças MIT/Apache-2.0
incluídas. O restante da biblioteca permanece no mesmo estado da versão fixada.

Alterações locais:

- `text/windows_font.rs`: rasterização e métricas de Segoe UI por GDI no Windows.
  A face interna `Windows UI` foi escolhida explicitamente pelo aplicativo.
  Altura lógica 12 corresponde a LOGFONT -12, observado nos controles ativos
  do original por REA (`docs/reference/active-control-fonts-runtime.json`).
  O Tahoma observado antes era outra fonte criada na inicialização.
  Aplica a escala física do
  viewport; usa glifos hintados e avanço inteiro, com cache de atlas do epaint.
- `text/font.rs`: utiliza esse resultado quando disponível. As outras faces,
  inclusive Courier New e emojis, continuam no rasterizador original.
  Uma falha do GDI preserva a alternativa de contorno da biblioteca.
  A curva 0,55 do atlas é compensada apenas para a máscara já rasterizada pelo
  GDI, evitando engrossar os contornos uma segunda vez no envio da textura.
- `tessellator.rs`: sufixo f32 em um literal para o compilador Rust atual.
- `Cargo.toml`: dependência Windows específica para as declarações da API GDI.

Limite: o atlas continua monocromático. O hinting do Windows não implica
equivalência com a mistura RGB subpixel do ClearType usado pelos controles
nativos. Não declarar igualdade pixel a pixel sem comparação das capturas.

`ANYCOMPARE_FONT_RENDERER=outline` permite reproduzir o rasterizador anterior
em uma execução de diagnóstico. Não alterar preferências globais do Windows.
