# ADR-0044: Declarative spatial requirements and zone occlusion

Status: accepted. Extension requested by the owner; beyond the original P0 map scope.

## Decision

A zone gains `occlusion: { sight, effect }`, both false for existing campaigns.
GM and co-GM edit it through versioned `SetZoneOcclusion`; players cannot edit
or receive zone geometry. Zones already persist complete DTO JSON, so no SQL
migration is needed. Merging zones with different occlusion is rejected rather
than silently changing which parts block a ray. Opening a door is an ordinary
versioned edit of its blocker, or deletion of its zone.

An action declares `spatial: { feature?, sight, line_of_effect,
area_line_of_effect }`. Missing spatial requirements preserve old behavior.
Feature references are validated. Area requirements are only accepted for area
actions. There are no system-specific spell names in the core.

The authority checks source-to-target (or selected area origin) rays before any
cost. Area targets additionally check origin-to-target rays before rolls, damage
and maintained effect attachment. A blocking change during an interruption is
rechecked on execution: an invalid root does not spend deferred costs, but costs
already paid at declaration remain spent. Interruption offers are revalidated
against their action's requirements. Movement reactions check the reach-exit
position, and check again when accepted. An obsolete movement offer may still
be declined so that movement can continue.

Filled rectangles, concave polygons and unions use integer segment intersections
with i128 products. Borders, tangencies, endpoint contact and origins/targets
inside blockers block a ray. Only obstacles on the same map participate. Sight
and effect are independent: smoke can block sight without stopping an effect;
glass can stop an effect while allowing sight.

## Limits

This is a conservative 2D center-to-center model. It does not trace token corners,
assign partial cover bonuses, account for height, light, senses or illusion rules,
change publication visibility, draw dynamic fog or prevent token movement.
Manual fog is not a physical blocker. The map's GM zone editor provides the
geometry; a plugin declares whether an action requires each test. The SRD example
has a campaign switch, with explicit limits in its README. Special actions may
omit requirements or use a separate declaration.

Client area indicators are geometric candidates; the authority filters the final
results. They cannot predict secret blockers. Rejections expose no obstacle name
or geometry. Player snapshots and events continue redacting zones.

Protocol 17 adds the intent and DTO field. SRD 0.9.0 enables sight for the two
reaction examples and effect paths for attacks and spells. Shape-only campaigns
continue behaving as before until blockers are enabled by their GM.

## Validation

Tests cover exact intersections, concave geometry, independent channels, a
blocked cast without costs, origin-to-target area filtering, reach-exit sight,
changed pending windows, feature disabling, invalid declarations, role/version
checks, player redaction, DTO persistence and UI submission of independent flags.
