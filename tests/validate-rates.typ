#import "/templates/detailed.typ": render as detailed
#import "/templates/summary.typ": render as summary
#let original = json("/tests/report-snapshot.json")
#let with-rate = original + (estimate: (
  rateText: "USD 80.00/h",
  amountText: "USD 120.00",
))
#detailed(with-rate)
#pagebreak()
#summary(with-rate)
#pagebreak()
#detailed(original + (estimate: none))
#pagebreak()
#summary(original + (estimate: none))
