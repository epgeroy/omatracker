#let render(data) = {
  let inv = data.invoice
  let accent = rgb(data.project.accentColor)
  set page(paper: data.project.paper, margin: 18mm,
    footer: context align(right)[#inv.number · #counter(page).display("1")])
  set text(size: 10pt)
  if data.project.at("logoPath", default: "") != "" {
    image(data.project.logoPath, width: 30mm)
    v(8pt)
  }
  text(size: 28pt, weight: "bold", fill: accent)[INVOICE]
  v(4pt)
  if inv.state == "draft" { text(fill: red, weight: "bold")[DRAFT — NOT ISSUED] }
  else { text(size: 14pt, weight: "bold")[#inv.number] }
  v(14pt)
  grid(columns: (1fr, 1fr), gutter: 18pt,
    [*From* \ #data.issuer.name \ #data.issuer.address \ #data.issuer.email \ #data.issuer.registrationId],
    [*Bill to* \ #data.client.name \ #data.client.address \ #data.client.email \ #data.client.registrationId])
  v(14pt)
  [*Project:* #data.project.name]
  linebreak()
  [*Service period:* #inv.from — #inv.to (end exclusive)]
  linebreak()
  if inv.issueDate != "" { [*Issued:* #inv.issueDate #h(12pt) *Due:* #inv.dueDate] }
  v(16pt)
  table(columns: (1fr, auto, auto, auto), inset: 8pt, stroke: 0.5pt + luma(80%),
    table.header([*Description*], [*Time*], [*Rate/hour*], [*Amount*]),
    ..data.lines.map(line => (line.task, line.duration, line.hourlyRate + " " + inv.currency, line.amountText)).flatten())
  v(12pt)
  align(right, text(size: 18pt, weight: "bold", fill: accent)[Total: #inv.totalText])
  v(20pt)
  if inv.paymentInstructions != "" { [*Payment instructions*]; linebreak(); text(inv.paymentInstructions) }
}
