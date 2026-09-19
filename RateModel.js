.pragma library

// Cosmetic interpolation between backend snapshots. Split whole hours from
// remaining seconds to avoid a large floating-point multiplication. Durable
// amounts and report rounding are always calculated by Rust.
function estimateText(rate, estimate, seconds) {
  if (!rate || !estimate) return ""
  var total = Math.max(0, Math.floor(seconds))
  var minor = Math.floor(total / 3600) * rate.amountMinor
    + Math.floor(((total % 3600) * rate.amountMinor + 1800) / 3600)
  if (!Number.isSafeInteger(minor)) return estimate.amountText
  var digits = estimate.fractionDigits
  var text = String(minor)
  if (digits > 0) {
    while (text.length <= digits) text = "0" + text
    text = text.slice(0, -digits) + "." + text.slice(-digits)
  }
  return estimate.currency + " " + text
}
