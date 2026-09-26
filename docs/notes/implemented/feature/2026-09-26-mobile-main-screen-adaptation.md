# Agent Note: Mobile main screen adaptation

Status: implemented

## Problem

The desktop conversation and settings layouts relied on a persistent sidebar and fixed-width controls, which made the first mobile screen cramped or prone to overflow.

## Decision

Use a 700px breakpoint for the main shell: move conversation navigation into a drawer and present settings categories in a horizontally scrollable tab bar. Constrain mobile dialogs to the available viewport, let the message search field fill remaining width, and stack knowledge-base import actions below 560px.

## Alternatives considered

- Keep the desktop sidebar and shrink it: rejected because it consumes too much of a phone viewport.
- Build a separate mobile navigation hierarchy: deferred because it would duplicate desktop behavior and increase maintenance cost.

## Consequences

Desktop layouts remain unchanged while narrow screens gain usable navigation and avoid the known fixed-width overflows. More detailed editing controls inside individual knowledge pages may still need follow-up tuning after testing on real phone-sized viewports.
