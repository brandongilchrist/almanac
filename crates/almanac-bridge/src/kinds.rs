//! Almanac Nostr event kinds (48050–48054).
//!
//! These mirror the allocation described in `docs/10_PLAN.md` § "Event kinds".
//! Each kind carries **parameterized-replaceable semantics by convention**
//! (keyed by `(pubkey, kind, d_tag)`; latest `created_at` wins), matching how
//! Buzz treats its 30000-range workflow kinds even when the bare number falls
//! outside NIP-33's strict `30000..=39999` window. Almanac applies the LWW
//! rule itself; callers should always emit a `d` tag equal to the entity id.
//!
//! | Kind  | Constant                  | Purpose                                          |
//! |-------|---------------------------|--------------------------------------------------|
//! | 48050 | `KIND_ALMANAC_SCHEDULE`   | A cron definition as seen by Almanac.            |
//! | 48051 | `KIND_ALMANAC_RUN`        | One concrete execution of a schedule.            |
//! | 48052 | `KIND_ALMANAC_MANIFEST`   | An artifact's materialization record (lineage).  |
//! | 48053 | `KIND_ALMANAC_CONTRACT`   | A producer/consumer dependency declaration.      |
//! | 48054 | `KIND_ALMANAC_CALENDAR`   | Calendar grouping/metadata.                      |

/// A cron definition as seen by Almanac. Mirror of a workflow def with
/// calendar-render hints (color, summary template, calendar subgroup).
/// NIP-33 `d` tag = schedule id.
pub const KIND_ALMANAC_SCHEDULE: u32 = 48050;

/// One concrete execution of a schedule. Carries `scheduled_for`,
/// `started_at`, `finished_at`, `status`, output manifest pointer.
/// NIP-33 `d` tag = run id.
pub const KIND_ALMANAC_RUN: u32 = 48051;

/// An artifact's materialization record. Carries `producer_run`,
/// `content_hash`, `schema_id`, `schema_version`, `commit_sha`, `uri`,
/// `bytes`, `materialized_at`. **The lineage primitive.**
/// NIP-33 `d` tag = `<run_id>:<schema_id>`.
pub const KIND_ALMANAC_MANIFEST: u32 = 48052;

/// A producer/consumer contract: declares what a schedule *expects to
/// produce* or *expects to consume* (schema id + version). The static
/// dependency declaration. NIP-33 `d` tag = contract id.
pub const KIND_ALMANAC_CONTRACT: u32 = 48053;

/// Calendar grouping/metadata: name, color, description, which schedules
/// belong. NIP-33 `d` tag = calendar id.
pub const KIND_ALMANAC_CALENDAR: u32 = 48054;

/// Lower bound of the Almanac kind range (inclusive).
pub const ALMANAC_KIND_MIN: u32 = 48050;

/// Upper bound of the Almanac kind range (inclusive).
pub const ALMANAC_KIND_MAX: u32 = 48054;

/// Returns `true` if `kind` is an Almanac-managed kind (48050–48054).
pub const fn is_almanac_kind(kind: u32) -> bool {
    matches!(
        kind,
        KIND_ALMANAC_SCHEDULE
            | KIND_ALMANAC_RUN
            | KIND_ALMANAC_MANIFEST
            | KIND_ALMANAC_CONTRACT
            | KIND_ALMANAC_CALENDAR
    )
}

/// Returns `true` if `kind` is within the broader Almanac range
/// (48050–48099), used for forward-compatible filtering.
pub const fn in_almanac_range(kind: u32) -> bool {
    kind >= ALMANAC_KIND_MIN && kind <= 48099
}

/// Returns `true` if `kind` is the lineage primitive (a manifest).
pub const fn is_manifest_kind(kind: u32) -> bool {
    kind == KIND_ALMANAC_MANIFEST
}

// Compile-time: all constants are distinct and in range.
const _: () = assert!(is_almanac_kind(KIND_ALMANAC_SCHEDULE));
const _: () = assert!(is_almanac_kind(KIND_ALMANAC_RUN));
const _: () = assert!(is_almanac_kind(KIND_ALMANAC_MANIFEST));
const _: () = assert!(is_almanac_kind(KIND_ALMANAC_CONTRACT));
const _: () = assert!(is_almanac_kind(KIND_ALMANAC_CALENDAR));
const _: () = assert!(KIND_ALMANAC_SCHEDULE < KIND_ALMANAC_RUN);
const _: () = assert!(KIND_ALMANAC_RUN < KIND_ALMANAC_MANIFEST);
const _: () = assert!(KIND_ALMANAC_MANIFEST < KIND_ALMANAC_CONTRACT);
const _: () = assert!(KIND_ALMANAC_CONTRACT < KIND_ALMANAC_CALENDAR);
const _: () = assert!(ALMANAC_KIND_MIN == 48050);
const _: () = assert!(ALMANAC_KIND_MAX == 48054);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_kinds_in_range_and_distinct() {
        let kinds = [
            KIND_ALMANAC_SCHEDULE,
            KIND_ALMANAC_RUN,
            KIND_ALMANAC_MANIFEST,
            KIND_ALMANAC_CONTRACT,
            KIND_ALMANAC_CALENDAR,
        ];
        assert_eq!(kinds.len(), 5, "exactly five almanac kinds");
        // Distinct.
        let mut sorted = kinds.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 5, "no duplicate kind numbers");
        // In range.
        for k in kinds {
            assert!(is_almanac_kind(k));
            assert!(in_almanac_range(k));
        }
    }

    #[test]
    fn boundaries() {
        assert!(!is_almanac_kind(48049));
        assert!(!is_almanac_kind(48055));
        assert!(in_almanac_range(48099));
        assert!(!in_almanac_range(48100));
        assert!(!in_almanac_range(48049));
    }

    #[test]
    fn manifest_predicate() {
        assert!(is_manifest_kind(KIND_ALMANAC_MANIFEST));
        assert!(!is_manifest_kind(KIND_ALMANAC_RUN));
    }
}
