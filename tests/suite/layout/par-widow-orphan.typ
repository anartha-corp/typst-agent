// Test fine-grained widow and orphan control via `par(widows: ...)` and
// `par(orphans: ...)`. Pages fit exactly four lines of text.

--- par-widows-default paged ---
// A 5-line paragraph on 4-line pages: the last two lines move together.
#set page(height: 52.5pt, margin: 0pt)
#lorem(2) #linebreak() #lorem(2) #linebreak() #lorem(2) #linebreak() #lorem(2) #linebreak() #lorem(2)

--- par-widows-three paged ---
// With `widows: 3`, the last three lines move together.
#set page(height: 52.5pt, margin: 0pt)
#set par(widows: 3)
#lorem(2) #linebreak() #lorem(2) #linebreak() #lorem(2) #linebreak() #lorem(2) #linebreak() #lorem(2)

--- par-widows-one paged ---
// With `widows: 1`, widow prevention is disabled and a single line may
// end a page.
#set page(height: 52.5pt, margin: 0pt)
#set par(widows: 1)
#lorem(2) #linebreak() #lorem(2) #linebreak() #lorem(2) #linebreak() #lorem(2) #linebreak() #lorem(2)

--- par-orphans-default paged ---
// The second paragraph starts mid-page; by default its first line moves
// to the next page so that no line starts a page on its own.
#set page(height: 52.5pt, margin: 0pt)
#lorem(2) #linebreak() #lorem(2) #linebreak() #lorem(2)

#lorem(2) #linebreak() #lorem(2) #linebreak() #lorem(2) #linebreak() #lorem(2) #linebreak() #lorem(2)

--- par-orphans-one paged ---
// With `orphans: 1`, a single line may start a page.
#set page(height: 52.5pt, margin: 0pt)
#set par(orphans: 1)
#lorem(2) #linebreak() #lorem(2) #linebreak() #lorem(2)

#lorem(2) #linebreak() #lorem(2) #linebreak() #lorem(2) #linebreak() #lorem(2) #linebreak() #lorem(2)
