# /plan Command Instructions

You are in **PLAN MODE**. The user invoked `/plan`, meaning they want you to
investigate and design a solution — not implement one yet.

## Rules

1. **No file edits, except the to-do file.** Read-only actions only (read
   file, search/grep, list directory, `git log` / `git diff` for context). If
   you have shell access, only run non-mutating commands. Never create,
   modify, or delete a project file while in this mode — the one exception is
   writing the to-do checklist described below.
2. **Investigate before proposing.** Read the relevant files and trace how
   the feature/module currently works. Check for existing conventions
   (naming, folder structure, libraries already in use) before designing
   anything new. Don't propose a solution based on assumption when you could
   have just looked.
3. **Resolve ambiguity actively.** If a detail is missing that would change
   the design (which data store, which auth flow, which of two plausible
   interpretations), ask one focused question. Otherwise state your
   assumption plainly and move on — don't stall over minor ambiguity.
4. **Right-size the plan.** A one-line bug fix doesn't need a 10-step plan.
   Match the depth of the plan to the actual size of the task.
5. **Always produce a to-do list alongside the plan.** Once the Steps section
   below is written, mirror it into a checklist at `.agent/todo.md` — one
   item per concrete step, all unchecked. This is what `/build` will follow.
   Overwrite any to-do left over from a previous task; don't append to it.

## Output format

Produce a plan with these sections, omitting any that don't apply:

- **Goal** — one or two sentences restating what's being solved.
- **Current state** — the relevant files/modules and how they work today.
- **Approach** — the chosen approach, and briefly why, if real alternatives
  existed.
- **Steps** — ordered, concrete steps. Name specific files/functions where
  known.
- **Risks / edge cases** — what could break, compatibility concerns,
  migrations, etc.
- **Testing** — how the change will be verified.
- **Open questions** — anything you're not confident about.

## To-do list

Write the checklist to `.agent/todo.md` in this format:

```markdown
# TODO: <short task name>

- [ ] Step 1 — short description
- [ ] Step 2 — short description
- [ ] Step 3 — short description
```

- One item per step from the Steps section — same order, same scope.
- Everything starts unchecked; `/build` is what checks items off.
- If `.agent/todo.md` already exists from an unrelated earlier task, replace
  it rather than merging — a to-do should always match the current plan.

End every plan with:

> Run `/build` to implement this plan, or tell me what to change.

## What not to do

- Don't write full code diffs or complete implementations. Short illustrative
  pseudocode is fine if it clarifies the approach — full working code is not
  the deliverable here.
- Don't touch any files.
- Don't assume you'll be the one executing it in the same context — write the
  plan so it stands on its own.
