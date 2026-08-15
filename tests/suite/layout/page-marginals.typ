// Test hiding page marginals (headers and footers) on empty pages.

--- page-marginals-parity paged ---
// The page inserted by `pagebreak(to: "odd")` is empty, so its header and
// footer are hidden. Pages with content keep them.
#set page(
  height: 80pt,
  margin: (top: 20pt, bottom: 20pt),
  marginals: "hide-empty",
  header: [HEADER],
  footer: [FOOTER],
)
#lorem(6)
#pagebreak(to: "odd")
#lorem(6)

--- page-marginals-always paged ---
// By default, marginals are shown on all pages, including empty ones
// inserted by `pagebreak(to: "odd")`.
#set page(
  height: 80pt,
  margin: (top: 20pt, bottom: 20pt),
  header: [HEADER],
  footer: [FOOTER],
)
#lorem(6)
#pagebreak(to: "odd")
#lorem(6)

--- page-marginals-numbering paged ---
// The page number is a marginal and is hidden on empty pages, but the page
// counter still counts them: the third page shows "3".
#set page(
  height: 80pt,
  margin: (top: 20pt, bottom: 20pt),
  marginals: "hide-empty",
  numbering: "1",
)
#lorem(6)
#pagebreak(to: "odd")
#lorem(6)

--- page-marginals-background paged ---
// The background is not a marginal: it is shown on the empty page while
// the header is hidden.
#set page(
  height: 80pt,
  margin: (top: 20pt, bottom: 20pt),
  marginals: "hide-empty",
  header: [HEADER],
  background: [BG],
)
#lorem(6)
#pagebreak(to: "odd")
#lorem(6)

--- page-marginals-trailing paged ---
// A trailing empty page after `pagebreak(weak: false)` also hides its
// marginals.
#set page(
  height: 80pt,
  margin: (top: 20pt, bottom: 20pt),
  marginals: "hide-empty",
  header: [HEADER],
)
#lorem(6)
#pagebreak(weak: false)
