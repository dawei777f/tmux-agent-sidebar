# Mascot feature page (website)

## Goal

Add a docs page that explains the sidebar mascot — the cat that lives at the
bottom of the sidebar and animates in response to agent activity. Surface it
under the existing **Features** sidebar group so it sits alongside the other
visible-from-the-UI features (agent pane, worktree, activity log, git status,
notifications).

The page should match the format of the existing feature pages: hero
screenshot at the top, short intro paragraph, then per-element subsections
with images.

## Out of scope

- Changes to the mascot implementation itself (`src/ui/mascot.rs`).
- Animated demos in the docs (GIF / WebM). Static screenshots only for v1.
- A separate marketplace / promo page for the mascot.

## Page

Path: `website/src/content/docs/features/mascot.mdx`

Structure:

```
---
title: Mascot
description: A short, evergreen blurb (under 160 chars). Used as og:description.
---

[hero image: mascot in Working state — reuses mascot-working.png]

A short paragraph explaining:
- what the mascot is (a small ASCII cat that lives in a 5-row band between
  the agent list and the bottom panel),
- that the mascot is opt-in via `@sidebar_mascot on` (link to
  reference/tmux-options),
- that its state is driven by the running-agent count (the same count
  surfaced in the status filter row).

## States

### Idle
[mascot-idle.png]
1-2 sentences: 0 running agents → cat sits at the home position. Periodically
plays a subtle motion (blink, wave, or a small jump) seeded per cycle.

### Walking
[mascot-walking.png]
1-2 sentences: when running count crosses 0 → ≥1 (or back), the cat walks
between its home position and the desk. Covers both `WalkRight` and
`WalkLeft` (mirrored sprites — the docs treat both as one "Walking" state
to avoid splitting on a directionality detail).

### Working
[mascot-working.png]
1-2 sentences: ≥1 running agent → cat sits at the desk shuffling papers.
Stack height grows with the running-agent count: 1 paper at 1 running,
2 papers at 2-3, capped at 2 for 4+. Describe in the soft "more agents,
taller stack" form, but mention the `MAX_PAPER_HEIGHT = 2` cap.
```

Tone & length: match the existing feature pages — terse, descriptive, no
marketing voice. Use `import { Image } from 'astro:assets';` and the same
`densities={[1.5, 2]}` pattern as `agent-pane.mdx`.

## Sidebar wiring

`website/astro.config.mjs`, in the `Features` group, append:

```js
{ slug: 'features/mascot' },
```

as the 5th entry (after `notifications`).

## Reference page update

`website/src/content/docs/reference/tmux-options.md` currently documents
`@sidebar_bottom_height`, `@sidebar_auto_create`, etc. but does not list
`@sidebar_mascot`. Add a row for `@sidebar_mascot` (`on` / `off`, default
`off`) so the link from the mascot page lands on real content instead of a
broken anchor.

## Capture pipeline

### Prerequisites (load-bearing)

The mascot has two render gates that the existing scenarios don't satisfy:

1. **`bot_h > 0`**: `src/ui/mod.rs:60-88` only allocates the mascot band
   when the bottom panel exists. Setting `BOTTOM_HEIGHT=0` would *hide*
   the mascot — do **not** override `BOTTOM_HEIGHT` for mascot scenarios.
2. **`@sidebar_mascot on`**: `src/ui/mod.rs:42-52` reads this tmux global,
   defaulting to `false`. Each mascot scenario must explicitly enable it
   before `start_sidebar`.

To keep individual scenarios small, add a helper to
`fixtures/scenarios/common/_lib.sh`:

```bash
# Enable the sidebar mascot. Must be called before start_sidebar so the
# initial @sidebar_mascot read picks it up.
enable_mascot() {
    tmux set-option -g @sidebar_mascot on
}
```

### Scenarios

Three new static scenarios under `fixtures/scenarios/`:

1. `mascot-idle/scenario.sh`
2. `mascot-walking/scenario.sh`
3. `mascot-working/scenario.sh`

Each scenario shares the same skeleton:

- Source `common/_lib.sh`.
- `setup "<scenario-name>"`, `trap cleanup EXIT`.
- `build_layout` to produce the standard 4-pane layout.
- Set crop env vars (see "Crop region" below).
- `enable_mascot`.
- Per-state setup (see table below).
- `start_sidebar`, sleep, `capture_single`.

### State control

`build_layout` seeds `MAIN_PANE` and `PANE_RUNNING_2` as `running` and
`PANE_WAITING` / `PANE_ERROR` as non-running. The mascot's running count
is computed from `@pane_status` on every pane (`tick_mascot` at
`src/state.rs:1126-1132`), so post-`build_layout` overrides via
`tmux set-option -t <pane_id> -p @pane_status …` are sufficient — no need
to fork or refactor `build_layout`.

Geometry of the walk (sidebar width = 46, all values from `src/state.rs:1135-1138`):

- `stop_x = panel_width − (DESK_OFFSET + DESK_WIDTH + CHAIR_WIDTH + 3) = 46 − (0 + 4 + 2 + 3) = 37` (the parked column where the cat sits).
- Distance from home: `stop_x − MASCOT_HOME_X = 37 − 1 = 36` columns.
- Step rule (`walk_step` in `src/state.rs:1140-1142`): 2 cols/tick while remaining > 8, then 1 col/tick. Total ticks: `(36 − 8) / 2 + 8 = 22`, at 200 ms/tick ⇒ **walk takes ~4.4 s**.
- `start_sidebar` (`fixtures/scenarios/common/_lib.sh:299`) sleeps 2 s after launching the binary. During that 2 s, ~10 ticks fire — tick 1 transitions Idle → WalkRight, ticks 2–10 advance `mascot_x` by 2 cols/tick. So `start_sidebar` returns with the cat already at roughly `mascot_x ≈ 20` (mid-walk), not at home.

| Scenario          | Setup after `build_layout`                                                                                          | Additional sleep after `start_sidebar` |
|-------------------|---------------------------------------------------------------------------------------------------------------------|----------------------------------------|
| `mascot-idle`     | Override both running panes to `waiting`: `tmux set-option -t "$MAIN_PANE" -p @pane_status waiting` (same for `$PANE_RUNNING_2`). The cat briefly takes a few right-walking steps before the next 1 s `refresh_interval` (`src/app.rs:46`) picks up the override and `running_count` drops to 0; the cat then walks back to `MASCOT_HOME_X = 1`. With ~2 s extra sleep (≈3 s past `enable_mascot`), the round trip completes well before capture. | ~2 s |
| `mascot-walking`  | Use `build_layout` defaults (2 running agents). Cat is already at `mascot_x ≈ 20` when `start_sidebar` returns; another ~12 ticks (~2.4 s) reach the desk. Sleep ~1 s lands the cat at `mascot_x ≈ 30`, well clear of both endpoints. | ~1 s                                   |
| `mascot-working`  | Use `build_layout` defaults (2 running agents). Need >2.4 s extra after `start_sidebar` to ensure the state has flipped to `Working`. Add headroom for paper-shuffle frame stability. | ~5 s                                   |

For the Idle override, only `@pane_status` matters for the mascot's
running-count (`tick_mascot` filters `PaneStatus::Running`, computed from the
live `repo_groups.panes[*].status` field). `@pane_attention` and
`@pane_wait_reason` set by `_seed_pane` are irrelevant to the mascot and
don't need to be cleared.

### Crop region

- `CROP_COLS=0:46` (full sidebar width — same as `agent-pane-focus`).
- `CROP_ROWS=21:26` — trim to the mascot scene band. With canvas height 46
  (set by `build_layout`), default `BOTTOM_PANEL_HEIGHT = 20`, and
  `MASCOT_SCENE_HEIGHT = 5` (`src/ui/mod.rs:23`), the band occupies rows
  `46 − 20 − 5 = 21` through `25` (0-indexed inclusive, 5 rows). The crop
  is end-exclusive per `_lib.sh` convention, so `21:26` selects exactly
  those rows. Verify against the first rendered HTML and adjust only if
  the layout constants change.

### Pipeline registration

`scripts/build-assets.sh`, after the existing `render_static` calls:

```bash
render_static mascot-idle
render_static mascot-walking
render_static mascot-working
```

Image assets land in `website/src/assets/captures/` and are imported from the
mdx page via `astro:assets`. The hero image at the top of the page reuses
`mascot-working.png` (same import) — no separate hero asset.

## Risks & open questions

- **Walking is transient.** The `mascot-walking` capture is timing-sensitive:
  total walk duration is ~4.4 s (22 ticks at 200 ms each, see "Geometry of
  the walk" above). The cat is already mid-walk when `start_sidebar` returns,
  and the recommended ~1 s extra sleep targets `mascot_x ≈ 30` (well clear
  of both home and the desk). If the captured frame is consistently at home
  or already at the desk, the fallbacks are:
  1. Add a debug-only env var (e.g. `TMUX_AGENT_SIDEBAR_FORCE_MASCOT_STATE=walking`)
     that pins `MascotState` and `mascot_x` for capture purposes. Most
     deterministic; small change in `src/app/setup.rs`.
  2. Switch the Walking image to a short animated capture
     (`capture_loop`) and pick a frame manually.
  3. Drop Walking from the docs page and only show Idle + Working.

  v1 ships with the static-frame approach; revisit only if the captured
  frame proves unstable.

- **Crop range needs an empirical tighten.** Document per scenario in a
  shell comment; rerun `scripts/build-assets.sh` until the band looks
  right. Plan should include a "render once, inspect, adjust" step.

- **Sidebar order.** Mascot is the most cosmetic of the Features entries,
  so placing it last in the group is intentional. If the user later wants
  it higher (e.g. just under `agent-pane`), reordering is a one-line
  change in `astro.config.mjs`.

## Success criteria

- `scripts/build-assets.sh` produces `mascot-idle.png`, `mascot-walking.png`,
  and `mascot-working.png` in `website/src/assets/captures/` without errors.
- The Astro build (`npm run build` inside `website/`) succeeds.
- The new `Mascot` entry appears in the Features sidebar on the rendered
  site, with a working hero image and three legible state thumbnails that
  visibly differ (cat at home vs. mid-walk vs. at the desk with papers).
- The page reads consistently with the other feature pages — no
  marketing-voice drift, no inconsistent heading depth.
