// Test continuation captions for breakable figures and the underlying
// block continuation mechanism.

--- figure-continuation-table paged ---
// A long table in a figure: the first page carries the caption, later
// pages carry the continuation caption at the top.
#set page(height: 70pt, margin: 0pt)
#figure(
  table(
    columns: (1fr, 1fr),
    table.header([Col A], [Col B]),
    ..range(24).map(x => (str(x), str(x * 2))).flatten(),
  ),
  caption: [TABEL 1: DATA],
  caption-repeat: [TABEL 1: DATA (LANJUTAN)],
)

--- figure-continuation-block paged ---
// Continuation captions also apply to other breakable figure bodies.
#set page(height: 70pt, margin: 0pt)
#figure(
  lorem(60),
  caption: [GAMBAR 1: TEKS PANJANG],
  caption-repeat: [GAMBAR 1 (LANJUTAN)],
)

--- figure-continuation-no-caption paged ---
// Error: 1:2-4:2 `caption-repeat` requires a caption
// Hint: 1:2-4:2 set `caption` to repeat it on following pages
#figure(
  table(columns: 1, ..range(20).map(x => (str(x),)).flatten()),
  caption-repeat: [X],
)

--- figure-continuation-float paged ---
// Error: 1:2-6:2 `caption-repeat` is not available for floating figures
// Hint: 1:2-6:2 remove `placement` to allow the figure to break across pages
#figure(
  table(columns: 1, ..range(20).map(x => (str(x),)).flatten()),
  caption: [X],
  caption-repeat: [X],
  placement: top,
)

--- block-continuation paged ---
// The underlying block mechanism, without a figure.
#set page(height: 70pt, margin: 0pt)
#block(
  breakable: true,
  continuation: [*LANJUT*],
  lorem(60),
)
