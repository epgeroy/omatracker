#let render(data) = {
  let accent = rgb(data.project.accentColor)
  set page(
    paper: data.project.paper,
    margin: (top: 18mm, bottom: 18mm, left: 16mm, right: 16mm),
    footer: context align(right)[Page #counter(page).display("1")],
  )
  set text(size: 9pt)
  set par(leading: 0.65em)

  if data.project.at("logoPath", default: "") != "" {
    image(data.project.logoPath, width: 28mm)
    v(8pt)
  }

  align(left)[
    #text(size: 20pt, weight: "bold", fill: accent)[#data.project.name]
    #v(3pt)
    #text(size: 12pt)[#data.period.label]
    #text(fill: luma(80%))[#data.period.start - #data.period.end]
  ]

  v(12pt)
  grid(
    columns: (1fr, 1fr),
    gutter: 12pt,
    [*Client* \ #data.project.clientName],
    [*Prepared by* \ #data.project.companyName],
  )

  v(14pt)
  block(
    inset: 10pt,
    radius: 4pt,
    fill: luma(97%),
  )[
    #text(size: 9pt, fill: accent)[TOTAL TIME]
    #linebreak()
    #text(size: 20pt, weight: "bold")[#data.totalDuration]
  ]

  v(16pt)
  if data.entries.len() == 0 {
    align(center)[No time entries were recorded during this period.]
  } else {
    table(
      columns: (1.1fr, 1.6fr, 0.75fr, 0.75fr, 0.9fr),
      inset: 6pt,
      stroke: (x: luma(88%), y: luma(88%)),
      fill: (_, row) => if calc.even(row) { luma(97%) } else { none },
      table.header(
        table.cell(fill: accent)[#text(fill: white, weight: "bold")[Date]],
        table.cell(fill: accent)[#text(fill: white, weight: "bold")[Task]],
        table.cell(fill: accent)[#text(fill: white, weight: "bold")[Start]],
        table.cell(fill: accent)[#text(fill: white, weight: "bold")[End]],
        table.cell(fill: accent)[#text(fill: white, weight: "bold")[Duration]],
      ),
      ..data.entries.map(entry => (
        [#entry.date],
        [#entry.task],
        [#entry.started],
        [#entry.ended],
        align(right)[#entry.duration],
      )).flatten(),
    )
  }
}
