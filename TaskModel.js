// Presentation-only helpers. The Rust CLI owns ledger parsing, time slicing,
// persistence, reports, and Drive synchronization.

function pad2(value) {
  var n = Math.max(0, Math.floor(Number(value) || 0))
  return n < 10 ? "0" + n : String(n)
}

function formatDuration(seconds) {
  var total = Math.max(0, Math.floor(Number(seconds) || 0))
  var hours = Math.floor(total / 3600)
  var minutes = Math.floor((total % 3600) / 60)
  return pad2(hours) + ":" + pad2(minutes) + ":" + pad2(total % 60)
}

function indexOfId(list, id) {
  if (!Array.isArray(list)) return -1
  for (var i = 0; i < list.length; i++) if (list[i] && list[i].id === id) return i
  return -1
}
