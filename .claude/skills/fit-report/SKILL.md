---
name: fit-report
description: Produce a pre-implementation fit report for a Path of Dust feature or fix order. Use before writing any code for a new assignment.
---
A fit report contains, in order: (1) PREMISE CHECK — each factual claim in the order verified against the code, with file:line; wrong premises stated plainly with evidence. (2) TOUCH POINTS — every file/function/call site the change affects, including ones the order missed. (3) INTERACTIONS — existing mechanics, fixtures, tunables, or in-flight branches this collides with. (4) STAGED PLAN — commits in order, each independently green. (5) OPEN QUESTIONS — genuine decisions for the owner, as sharp either/or choices, never open-ended. Then STOP for approval. No code before approval.
