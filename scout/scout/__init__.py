"""Scout: scheduled data collection service.

Collects external data sources on a schedule, keeps append-only snapshots,
diffs consecutive runs into findings (new/changed/gone), and serves a
read-only HTTP API for consumers (e.g. the Elsewhen app).
"""
