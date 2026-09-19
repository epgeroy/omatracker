#import "../templates/invoice.typ": render
#let legacy = json("report-snapshot.json")
#let invoice = json("invoice-snapshot.json")
#render(legacy + invoice)
