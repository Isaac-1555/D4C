# /build Command Instructions

You are in **BUILD MODE**. The user invoked `/build`, meaning they want the
change actually implemented.

## Rules

1. **Use the plan if one exists.** If a `/plan` was produced earlier in this
   session (or the user pasted one in), treat it as the source of truth and
   follow its steps in order.
   - NOTE ON WIRING: if plans and builds don't share conversation state in
     your setup, have `/build` check for a plan file (e.g. `.agent/plan.md`)
     before starting, and have `/plan` write to that same path.
   - If you discover mid-implementation that the plan is wrong or
     incomplete, say so explicitly and explain the deviation before
     proceeding. Don't silently go off-script.
2. **Always work from a to-do list.** Before making any edits:
   - Check for `.agent/todo.md`. If it exists, that's your working checklist
     — follow it top-to-bottom.
   - If it doesn't exist, create it first: derive one item per concrete step
     from the plan if you have one, or from your own quick breakdown of the
     request if you don't. Use the same format `/plan` uses:
     ```markdown
     # TODO: <short task name>
     - [ ] Step 1 — short description
     - [ ] Step 2 — short description
     ```
     Only start editing project files once this exists.
   - As you work, mark an item in-progress when you start it (`- [~]`) and
     done (`- [x]`) the moment it's finished — update the file after each
     item, not in one batch at the end. Progress should stay visible to
     someone reading the file mid-build.
   - If you discover a necessary step that wasn't listed, add it. If a listed
     step turns out unnecessary, mark it `- [skip] reason` rather than
     deleting it silently.
3. **Match existing conventions.** Follow the codebase's existing style,
   patterns, and libraries rather than introducing new ones, unless the plan
   calls for it.
4. **Stay in scope.** Touch only what's needed for the request. Don't
   refactor unrelated code, rename things, or "clean up" files that weren't
   part of the task.
5. **Verify your work.** After making changes, run the project's
   build/lint/test commands if available. If something fails, fix it before
   reporting done — never hand back a broken build.
6. **Guard irreversible actions.** Never run destructive or irreversible
   operations (force pushes, `rm -rf`, dropping/truncating data, overwriting
   migrations that already ran, etc.) without explicit confirmation from the
   user first — even if it seems like the fastest path.
7. **Pause at real forks in the road.** If you hit a design decision the plan
   didn't cover, or a change that would affect something outside the stated
   scope, stop and ask rather than guessing.

## Output format

After implementing, report:

- **Summary** — what changed, in plain language.
- **Files touched** — created / modified / deleted.
- **Verification** — what you ran (tests/build/lint) and the result.
- **To-do status** — final state of `.agent/todo.md` (all checked, or what's
  left and why).
- **Follow-ups** — anything left undone or suggested next steps, if any.
