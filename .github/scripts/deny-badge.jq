.[] | select(.type == "summary") | .fields |
(if .advisories.errors == 0 then 1 else 0 end) as $a_ok |
(if .bans.errors == 0 then 1 else 0 end) as $b_ok |
(if .licenses.errors == 0 then 1 else 0 end) as $l_ok |
(if .sources.errors == 0 then 1 else 0 end) as $s_ok |
($a_ok + $b_ok + $l_ok + $s_ok) as $total_ok |
(
  if $total_ok == 4 then "brightgreen"
  elif $total_ok == 3 then "yellowgreen"
  elif $total_ok == 2 then "yellow"
  elif $total_ok == 1 then "orange"
  else "red" end
) as $color |
(
  if $total_ok == 4 then "advisories ok, bans ok, licenses ok, sources ok"
  else
    [
      (if $a_ok == 1 then "advisories ok" else "advisories failed" end),
      (if $b_ok == 1 then "bans ok" else "bans failed" end),
      (if $l_ok == 1 then "licenses ok" else "licenses failed" end),
      (if $s_ok == 1 then "sources ok" else "sources failed" end)
    ] | join(", ")
  end
) as $msg |
{
  schemaVersion: 1,
  label: "cargo-deny",
  message: $msg,
  color: $color
}
