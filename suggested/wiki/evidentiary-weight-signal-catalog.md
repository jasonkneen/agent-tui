# The evidentiary-weight signal catalog — every signal that raises or lowers a drift value's weight

When two docs assert different values for one fact, the drift is adjudicated by weighing each value's *evidentiary posture*, not just its quote. The registration convention requires the row to record "the **evidentiary-weight signals** already visible for that value" — but it notes that "existing law supplies the signal vocabulary" without collecting it. This page is that collection: the complete signal set, what each does to a value's weight, and how to read it. Registrants score every value against this list at registration; adjudicators inherit the scores rather than re-deriving them.

## The signals

| Signal | Direction | How to detect it |
|---|---|---|
| **In a `(shipped)`-marked section** covering the disputed fact specifically | Raises — a claim inside a shipped-marked section outranks unmarked prose and older statements | The section carries the `(shipped)` marker **and** its own claims address the disputed fact. Silence on the fact lends nothing; proximity to an adjacent-subject sentence lends nothing. |
| **Phantom-grounding** (quote survives only in an earlier revision) | Lowers — downgraded below any value grounded in the source's *current* text | Re-open the cited source at its current revision; confirm the verbatim sentence is still present. If it is gone, the value is phantom-grounded. |
| **Attribution** (author + date present) | Raises — an attributed decision can serve as evidence later | The value's source carries an author and date. An unattributed value cannot serve as evidence in a later adjudication. |
| **Modification order** between the competing sources | Contextual — the later-modified source is a signal of currency, weighed with the others | Compare the modification timestamps of the two sources; record which moved last. |
| **Referent scope** (do both quotes describe the same fact?) | Gating — resolves whether there is a genuine conflict at all | Confirm both quotes assert values for one fact. If a quote's referent is ambiguous, register the drift but record the candidate partial reconciliation as a possible canonical outcome. |

## Scoping rules that constrain the signals

- **The `(shipped)` marker is section- and subject-scoped.** Its weight attaches only to the individually checkable claims the marked section itself makes about its own subject — never to the whole doc, never to an adjacent-subject sentence tucked inside for proximity, never to the section's silence on a fact. Crediting a marker outside this scope manufactures false evidence exactly as mislabeling a design intention does.
- **A rationalization is not a signal.** A doc's after-the-fact explanation of an inconsistency ("this was intentional design") carries no weight; it is probed like any other claim.
- **A stale claim under a `(shipped)` marker inverts the marker.** It is the highest-severity doc defect precisely because the elevated weight now backs a false claim — repair (fix or strip) takes priority over ordinary staleness.

## How to use it

**At registration:** for each competing value, walk the table and record every applicable signal in the row alongside the value's verbatim quote, source path, and timestamp. The signals are cheapest to capture now, while both sources are open; left uncaptured they must be re-derived later against sources that may have been edited again.

**At adjudication:** the row already carries the posture. Verify the ground-truth step (a wire probe where behavior is the fact), then let the recorded signals break ties among doc-only values. A value that is attributed, current-revision-grounded, and inside a subject-matching shipped section outweighs an unattributed, phantom-grounded, unmarked one.

**When a new signal is discovered,** add a row here and reference this catalog from the registration convention — the vocabulary lives in one place so registrants and adjudicators score against one checklist.
