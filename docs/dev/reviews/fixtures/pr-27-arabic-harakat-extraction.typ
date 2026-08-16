// Reproduction fixture for upstream issue #4225:
// "PDF text extraction can fail in complex shaping scenarios"
//
// Requires a system Arabic font (Noto Naskh Arabic UI). Run:
//
//   typst-agent compile pr-27-arabic-harakat-extraction.typ out.pdf
//   pdftotext out.pdf out.txt
//
// Observed on v0.15.1-era code: the harakat (combining marks) detach and
// reorder in the extracted text; for the source line
//   اقتباس من النص العربي: "صَحفيّة ودَوليّة" مع الحركات والتشكيل.
// pdftotext yields lines like
//   "َصحفّيّي ة َد
//   َص
//   وَدولّيّي ة" مع الحركات
// where "صَحفيّة" and "ودَوليّة" are scrambled and stray marks appear on
// their own lines. This breaks selectable, searchable, and
// copy-pasteable text for harakat-bearing Arabic (critical for
// repository/plagiarism extraction of STIK citations).

#set page(width: 160pt, height: 120pt, margin: 12pt)
#set text(lang: "ar", font: "Noto Naskh Arabic UI", size: 12pt)

اقتباس من النص العربي: “صَحفيّة ودَوليّة” مع الحركات والتشكيل.
