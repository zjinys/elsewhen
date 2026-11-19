"""Pluggable collection executors.

Each executor takes a parsed source spec dict and returns a normalized item
list (list of dicts, each with a stable "id" field). Executors are pure data
fetchers; snapshot/diff/scheduling live outside.
"""
