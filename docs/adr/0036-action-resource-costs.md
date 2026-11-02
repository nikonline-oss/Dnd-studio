# ADR 0036: Multiple action resource costs

Status: accepted.

Actions need to consume more than one resource, such as a turn action and a
spell slot. Packages declare a bounded list of resource IDs and positive integer
amount expressions. The interpreter resolves all amounts against the original
actor, aggregates repeated IDs and checks all balances before applying expenses
to its working copy. Subsequent expenses read the updated character so that
writes to different fields cannot overwrite each other. Rejection publishes no
partial events. The existing single-unit `cost` remains compatible; mixing both
forms is rejected. Protocol version 12 exposes the added optional data.

Damage transformation and general action interruptions are separate work.
