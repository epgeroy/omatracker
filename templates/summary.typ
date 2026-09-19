#let render(data) = {
  let accent = rgb(data.project.accentColor)
  set page(
    paper: data.project.paper,
    margin: (top: 24mm, bottom: 22mm, left: 20mm, right: 20mm),
    footer: align(center)[#data.project.name - #data.period.start to #data.period.end],
  )
  set text(size: 10pt)

  if data.project.at("logoPath", default: "") != "" {
    align(center, image(data.project.logoPath, width: 28mm))
    v(8pt)
  }

  align(center)[
    #text(size: 10pt, fill: accent, weight: "bold")[TIME REPORT]
    #v(8pt)
    #text(size: 25pt, weight: "bold")[#data.project.name]
    #v(4pt)
    #text(fill: luma(45%))[#data.period.start - #data.period.end]
    #v(18pt)
    #text(size: 34pt, weight: "bold", fill: accent)[#data.totalDuration]
    #v(3pt)
    #text(fill: luma(45%))[Total tracked time]
    #if data.at("estimate", default: none) != none {
      v(10pt)
      text(size: 10pt)[Hourly rate: #data.estimate.rateText]
      linebreak()
      text(size: 16pt, weight: "bold", fill: accent)[Estimated amount: #data.estimate.amountText]
    }
  ]

  v(24pt)
  line(length: 100%, stroke: 1pt + accent)
  v(12pt)
  grid(
    columns: (1fr, 1fr),
    gutter: 14pt,
    [*Client* \ #data.project.clientName],
    [*Prepared by* \ #data.project.companyName],
    [*Report period* \ #data.period.label],
    [*Entries* \ #str(data.entries.len())],
  )

  v(22pt)
  text(size: 12pt, weight: "bold")[Activity]
  v(6pt)
  if data.entries.len() == 0 {
    [No time entries were recorded during this period.]
  } else {
    for entry in data.entries {
      block(
        inset: (top: 6pt, bottom: 6pt),
        below: 2pt,
        stroke: (bottom: 0.5pt + luma(88%)),
      )[
        #grid(
          columns: (1fr, auto),
          [*#entry.task* \ #entry.date],
          [#entry.duration],
        )
      ]
    }
  }
}
