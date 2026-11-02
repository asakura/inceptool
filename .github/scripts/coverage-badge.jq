.data[0].totals.lines.percent as $p | {
  schemaVersion: 1,
  label: "coverage",
  message: "\(($p * 10 | round) / 10)%",
  color: (if $p >= 80 then "brightgreen" elif $p >= 60 then "yellow" else "red" end)
}
