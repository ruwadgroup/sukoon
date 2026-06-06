# Halal-aware modes

Sukoon's separation **modes** let users choose what counts as "music to remove." This is a genuine
differentiator — no competitor offers it — and it's handled with care.

> **Sukoon does not issue religious rulings.** These modes map to _documented scholarly positions_,
> each with a sourced explanation, so users choose informed. The default is the broadest common
> denominator, not an endorsement. See [GOVERNANCE.md](../../GOVERNANCE.md) and
> [design-considerations §5](../design-considerations.md#5-sukoon-presents-scholarly-positions--it-does-not-issue-rulings).

## The modes

**Spoken speech — dialogue, narration, lectures, recitation — is _always_ kept.** The modes differ
only in what they do with **instrumental music** and a **sung human voice**:

| Mode               | Instrumental music                           | Sung vocal melody (a background song's singing)                             | Maps to the position that…                                                           |
| ------------------ | -------------------------------------------- | --------------------------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| `remove-all`       | ❌ removed                                   | ❌ **removed** (the whole song goes, leaving only spoken speech)            | …music — instruments _and_ the sung melody of a song — is to be avoided. _(default)_ |
| `keep-percussion`  | ❌ removed (melodic); ✅ duff/hand-drum kept | ❌ removed                                                                  | …the duff is permitted while melodic/stringed/wind instruments are not.              |
| `keep-vocals`      | ❌ removed                                   | ✅ **kept** (a song becomes a cappella — instruments stripped, voice stays) | …the unaccompanied human voice is acceptable while instruments are not.              |
| `preserve-effects` | ❌ removed                                   | ❌ removed                                                                  | (practical) ambient sound/SFX aren't "music."                                        |

> **What ships today (be honest about this).** The working engine — **MDX-Net**, a 2-stem
> voice/instrumental separator — implements **remove-music / keep-voice**. So `remove-all` and
> `keep-vocals` both keep the voice and drop the instrumental, and on this engine they resolve to the
> **same behaviour**. `keep-percussion` and `preserve-effects` need a **multi-stem** engine to be
> meaningful and are **not implemented yet** — they remain documented positions and roadmap items.
> The table above is the design intent.

These map to `SeparationMode` in the core ([pipeline.rs](../../packages/core/src/pipeline.rs)).
The sourced positions are in [scholarly-positions.md](./scholarly-positions.md).

## Related controls (planned)

- **Sensitivity slider** — aggressive vs gentle removal (trades speech clarity vs music suppression).
- **Nasheed mode** — tuned thresholds for vocal-forward content with light backing, so recitation
  is never clipped.

## The hard case: nasheeds

When a recited/sung voice sits over instrumental backing, the voice we must keep and the music we
want to remove are acoustically entangled. Sukoon treats **preserving the voice as inviolable** and
errs toward keeping it, exposing the instrument side through these modes rather than risking damage
to recitation. A dedicated [eval corpus](../contributing/model-eval.md) covers this case.
