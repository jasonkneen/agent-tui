# Convention: warm-pool lifecycle obligations are shape-relative — idle timeout, health probe, and eager warm bind to the vendor's runtime shape, not to sockets

**Rule.** The three lifecycle behaviors every vendor runtime must exhibit — **idle timeout**, **health probe before reuse**, and **optional eager warm** — are obligations on the vendor's *warm-runtime shape*, not on a literal socket. Each shape realizes them differently, and a review must check for the shape-appropriate realization of all three:

| Obligation | Held socket (Codex) | Sticky subprocess session (Claude) |
|---|---|---|
| Idle timeout | Close the long-lived connection | Let the sticky session lapse rather than holding resources indefinitely |
| Health probe before reuse | Ping/noop, respawn if dead | Verify the `--resume` id still resolves; start a fresh session if it doesn't |
| Eager warm | Spawn the server on TUI start when the CLI is detected | Optionally establish the initial session early |

Two review errors are both defects: exempting a subprocess-shaped vendor from the lifecycle ("no socket, so the pool convention doesn't apply") and demanding literal socket mechanics from it (rejecting a correct `--resume`-validity check because it isn't a ping).

**Grounding.**
- `suggested/wiki/warm-runtime-shapes-socket-vs-subprocess.md`, "Why the distinction matters": "The warm-pool lifecycle convention (idle timeout, health probe, optional eager warm) was written against the socket exemplar, and its wording — 'one warm connection' — reads naturally only for Codex. The Claude row shows the same three lifecycle obligations map onto session shape differently", followed by the three per-shape mappings quoted in the table above.
- `suggested/agent-updates/claude-harness-sticky-resume.md`: sticky `--resume` amortizes "the costs ... for a subprocess-shaped vendor, exactly as the warm socket does for Codex" — the subprocess session is the same warm asset, so it carries the same lifecycle duties.
- `docs/LOCAL_CLI_AUTH.md`, "Why a warm connection": the cold-start cost model (process spawn, auth materialization + initialize handshake, TLS / first connect / tool index) is stated shape-neutrally — it is what all three obligations exist to manage.

**Why:** the existing pool convention ("vendor runtime pools keep one warm connection — idle timeout, health probe, optional eager warm; never per-request spawns") is correct but socket-worded, and the shapes wiki itself records that the wording "reads naturally only for Codex." As subprocess-shaped vendors ship (Claude is one today), that gap becomes operative: an implementer can honestly claim the pool convention doesn't describe their vendor, and a reviewer has no written basis to require session-lapse timeouts or resume-id health checks. Binding the obligations to the shape closes the gap without weakening the socket reading.

**How to apply:** when building or reviewing a vendor runtime, first identify its shape from the two-shapes table, then check that all three obligations are present *in that shape's realization* — a pool or harness missing any one is incomplete, whichever shape it is. When writing docs or specs for a new vendor, phrase the lifecycle requirements in the shape's own vocabulary (session lapse / resume validation / early session) rather than copying the socket wording.