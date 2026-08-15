// Reproduction fixture for upstream issue #8489:
// "PDF/A output incorrectly disallows combining marks across fonts/languages"
//
// Requires a system Arabic font (Noto Sans Arabic / Noto Naskh Arabic UI) in
// addition to the embedded Libertinus Serif. Run:
//
//   typst-agent compile pr-26-pdfa-combining-marks.typ out.pdf --pdf-standard a-3a
//
// Expected (broken) output on v0.15.1-era code:
//
//   error: PDF/A-3a error: the text `"0"` could not be displayed with font
//   `"Libertinus Serif"` ... hint: try using a different font
//
// The digit U+0030 is shaped with the embedded Libertinus Serif while the
// combining mark U+064B needs an Arabic font; the cluster attribution across
// the two runs triggers a krilla validation error. The same text compiles
// fine without `--pdf-standard` (no validation).

#set page(width: 120pt, height: 120pt, margin: 10pt)
#set text(size: 12pt)

Digit and Arabic mark across fonts: 0ً
