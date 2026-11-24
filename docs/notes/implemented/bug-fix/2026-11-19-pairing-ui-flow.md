# Agent Note: Pairing UI flow follows the user task

Status: implemented

## Problem

The pairing screen exposed protocol stages, removed the progress view after approval, required manual approval checks, and could show another invitation's completion or error state.

## Decision

The UI presents two explicit intents and three user-facing stages. Native phases remain implementation details. Owner approval stores the approved device identity and keeps transfer progress visible. Enrollment errors are scoped by operation and invitation. Joiner coordination retries request, approval inspection, installation confirmation, and proof delivery automatically while the session remains open. QR expiry and camera fallback are explicit. Roster copy describes connected devices because the roster is not an online presence signal.

## Alternatives considered

- Keep the existing protocol checklist: rejected because users had to interpret internal handoff phases and waiting actions.
- Hide progress until the transfer completes: rejected because approval and transfer are separate states and hiding transfer made the UI appear finished.
- Poll only from the widget: rejected because navigating away stopped pairing progress; timers belong to the controller lifecycle.

## Consequences

The pairing surface is shorter and actionable on a phone. Device identity and authorization remain visible before sharing. A small controller timer is active during pending coordination and is canceled on close. The existing secure approval and protected revision contracts remain unchanged.
