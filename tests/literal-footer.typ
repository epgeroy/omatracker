// Valid Typst syntax, deliberately broken markup-mode interpolation.
#let render(data) = {
  set page(footer: [
    text(size: 9pt)[data.issuer.name]
    h(1fr)
    text(size: 9pt)[data.invoice.totalText]
  ])
  [#data.client.name — #data.invoice.totalText]
}
