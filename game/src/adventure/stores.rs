// The store classification (2026-09-08).
//
// WHY THIS FILE EXISTS. A season reset had no representation in code at
// all: no reset function, no wipe, nothing that decided what a season
// destroys. It was `rm` by hand against a runbook. What survived a reset
// survived because nobody typed its filename - `adventure-accounts.json`
// is not named in the cutover steps, so accounts persist by OMISSION.
// That is convention, and the runbook already has a documented instance
// of a step being skipped in practice (`OPERATOR_BOOTSTRAP` was left set
// on live production through the World 2 reset).
//
// This table is the declaration that replaces the omission. Every
// persisted entry in the data directory is named here with a scope, so
// "does this survive a season?" is answered by a value in the codebase
// rather than by what an operator remembers at 1am.
//
// THE CHECK IS ONE-DIRECTIONAL, AND THAT IS DELIBERATE (ruling
// 2026-09-08). Only `present => declared` holds against a live data
// directory. The reverse - "every declared store exists" - is FALSE
// here and asserting it would break the guard the first time anyone
// used it:
//
//   Stores are created LAZILY, on first write. 31 tolerant loads
//   (`load_json` -> `Option`, `.unwrap_or(..)`) against 14 fail-loud,
//   and every marker is among the tolerant ones - a marker file does
//   not exist until its migration fires. On a freshly reset world
//   almost NONE of the world-scoped entries below exist yet: no
//   markers, no fight directories, no reforge cooldowns, no rampage
//   state. A check demanding presence would refuse to run on exactly
//   the world it was built to protect, and its failure would look
//   identical to real corruption.
//
// The contrast worth keeping, because it is why the sprite manifest's
// shape did NOT transfer: sprites are static files in a CHECKOUT, so
// both directions are knowable at test time. Data stores are runtime
// state created on demand. Generalising the mechanism past the property
// that made it work is the mistake this comment exists to prevent
// someone repeating.
//
// The bidirectional shape still lives in the COMPILE-TIME half, where
// it is true and free - see this module's tests: every entry carries a
// scope and a reason, no name is declared twice, and every bucket is
// populated. No directory is involved in any of that.

use std::collections::BTreeSet;
use std::path::Path;


/// What a season reset does to a store.
///
/// FOUR buckets, not two (ruling 2026-09-08). The two-bucket framing the
/// work started from - world or account - could not express the live
/// directory: forcing `adventure-live-tunables.toml` into "account"
/// would be false, and forcing `wiki/` into either invites a reset
/// deleting content the owner edits by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StoreScope {
    /// Destroyed by a season reset. This world's characters, its
    /// history, and the one-time markers recording what was applied to
    /// them.
    World,
    /// Survives a season reset. Player identity - who you are, as
    /// opposed to what you did last season.
    Account,
    /// Survives a season reset. Operator configuration and published
    /// output, not player state: tunables, balance, patch notes.
    Config,
    /// Not persisted state at all - shipped assets, live content and
    /// logs that happen to sit in the same directory.
    ///
    /// A real bucket rather than an omission: naming them explicitly is
    /// what stops a later reader classifying `wiki/` by accident, and
    /// what stops the reset guard refusing on entries that were never
    /// its business.
    NotAStore,
}

/// File or directory. The reset command needs to know which, because
/// removing a directory is a different call and a much larger mistake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreKind {
    File,
    Dir,
}

/// The facts about one store: where it lives, what a reset does to it,
/// and why.
///
/// Reached only through `Store::spec`, so every field is total - there is
/// no way to hold a spec for a store that has not been classified.
#[derive(Debug, Clone, Copy)]
pub struct StoreSpec {
    /// The entry name inside the data directory, exactly as it appears
    /// on disk.
    pub name: &'static str,
    pub scope: StoreScope,
    pub kind: StoreKind,
    /// Why it is classified this way, in one line.
    ///
    /// Printed by the reset command before it deletes anything - an
    /// operator must be able to see the classification it is acting on,
    /// not just the outcome afterwards.
    pub why: &'static str,
}


/// Every persisted entry the data directory may hold, as a TYPE
/// (2026-09-08).
///
/// **This is what makes the classification by construction rather than by
/// convention.** `data_path` takes one of these and nothing else - there
/// is no string-taking entry point left - so a new store cannot be
/// resolved to a path at all until it is a variant here, and `spec`'s
/// match is exhaustive, so a variant cannot exist without a scope and a
/// reason. Forgetting to classify a store is a compile error rather than
/// something discovered at the next reset.
///
/// The one operation that still takes a path is
/// `paths::normalize_caller_path`, which takes a `&Path` and CANNOT name
/// a store - see its doc for why that distinction is enforceable rather
/// than conventional.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Store {
    Characters,
    World,
    Sessions,
    ReforgeCooldown,
    SpriteCount,
    LastFights,
    LastFightsJsonBak,
    RampageState,
    FightsBundleSeq,
    FightsCoarseSeq,
    FightsDetailSeq,
    FightsSummarySeq,
    FightsBundle,
    FightsCoarse,
    FightsDetail,
    FightsSummary,
    FightsPinned,
    AffixTierCurveMarker,
    CelestialShardFirstAwardMarker,
    CelestialShardIntoUniqueShardMarker,
    CraftTokenBackfillMarker,
    CraftTokenBackfillV2Marker,
    CritFlagToAffixTrackingMarker,
    CritLineageBackfillMarker,
    CritReforgeEquippedBackfillMarker,
    CritValueNerfMarker,
    DuplicateUniqueEffectsCleanupMarker,
    FightsStorageMigrationMarker,
    FlowlikewaterSwapMarker,
    GlovesSpeedRebalanceMarker,
    HelmRebalanceV2Marker,
    ItemAccuracyMarker,
    KibukahCompensationMarker,
    KrangleAccuracyMarker,
    LingeringEffectToEchoMarker,
    PassiveKeyRenameMarker,
    PityLaunchMarker,
    PowerRollBackfillMarker,
    RefundRetiredDeadNodesMarker,
    StarterKitBackfillMarker,
    UniqueShardFirstAwardMarker,
    WingsGiveawayMarker,
    WingsLaunchGrantMarker,
    Accounts,
    LiveTunables,
    ItemBalance,
    PassiveOverrides,
    Bugreports,
    PatchNotes,
    BotPublishedConstants,
    Templates,
    Wiki,
    PublicAdventureOverlay,
    Logs,
}

impl Store {
    /// Every store, for the reset command and the table's own tests.
    ///
    /// `spec` is compiler-enforced, but this list is hand-maintained and is
    /// therefore the one place a new variant can silently go missing -
    /// `all_lists_every_variant_exactly_once` is what guards it.
    pub const ALL: &'static [Store] = &[
    Store::Characters,
    Store::World,
    Store::Sessions,
    Store::ReforgeCooldown,
    Store::SpriteCount,
    Store::LastFights,
    Store::LastFightsJsonBak,
    Store::RampageState,
    Store::FightsBundleSeq,
    Store::FightsCoarseSeq,
    Store::FightsDetailSeq,
    Store::FightsSummarySeq,
    Store::FightsBundle,
    Store::FightsCoarse,
    Store::FightsDetail,
    Store::FightsSummary,
    Store::FightsPinned,
    Store::AffixTierCurveMarker,
    Store::CelestialShardFirstAwardMarker,
    Store::CelestialShardIntoUniqueShardMarker,
    Store::CraftTokenBackfillMarker,
    Store::CraftTokenBackfillV2Marker,
    Store::CritFlagToAffixTrackingMarker,
    Store::CritLineageBackfillMarker,
    Store::CritReforgeEquippedBackfillMarker,
    Store::CritValueNerfMarker,
    Store::DuplicateUniqueEffectsCleanupMarker,
    Store::FightsStorageMigrationMarker,
    Store::FlowlikewaterSwapMarker,
    Store::GlovesSpeedRebalanceMarker,
    Store::HelmRebalanceV2Marker,
    Store::ItemAccuracyMarker,
    Store::KibukahCompensationMarker,
    Store::KrangleAccuracyMarker,
    Store::LingeringEffectToEchoMarker,
    Store::PassiveKeyRenameMarker,
    Store::PityLaunchMarker,
    Store::PowerRollBackfillMarker,
    Store::RefundRetiredDeadNodesMarker,
    Store::StarterKitBackfillMarker,
    Store::UniqueShardFirstAwardMarker,
    Store::WingsGiveawayMarker,
    Store::WingsLaunchGrantMarker,
    Store::Accounts,
    Store::LiveTunables,
    Store::ItemBalance,
    Store::PassiveOverrides,
    Store::Bugreports,
    Store::PatchNotes,
    Store::BotPublishedConstants,
    Store::Templates,
    Store::Wiki,
    Store::PublicAdventureOverlay,
    Store::Logs,
    ];

    /// This store's classification. Exhaustive by construction: a new
    /// variant does not compile until it has a name, a scope, a kind and
    /// a reason.
    pub const fn spec(self) -> StoreSpec {
        StoreSpec { name: self.name(), scope: self.scope(), kind: self.kind(), why: self.why() }
    }

    /// The entry's name on disk.
    pub const fn name(self) -> &'static str {
        match self {
            Store::Characters => "adventure-characters.json",
            Store::World => "adventure-world.json",
            Store::Sessions => "adventure-sessions.json",
            Store::ReforgeCooldown => "adventure-reforge-cooldown.json",
            Store::SpriteCount => "adventure-sprite-count.json",
            Store::LastFights => "adventure-last-fights.json",
            Store::LastFightsJsonBak => "adventure-last-fights.json.bak",
            Store::RampageState => "adventure-rampage-state.json",
            Store::FightsBundleSeq => "adventure-fights-bundle-seq.json",
            Store::FightsCoarseSeq => "adventure-fights-coarse-seq.json",
            Store::FightsDetailSeq => "adventure-fights-detail-seq.json",
            Store::FightsSummarySeq => "adventure-fights-summary-seq.json",
            Store::FightsBundle => "adventure-fights-bundle",
            Store::FightsCoarse => "adventure-fights-coarse",
            Store::FightsDetail => "adventure-fights-detail",
            Store::FightsSummary => "adventure-fights-summary",
            Store::FightsPinned => "adventure-fights-pinned",
            Store::AffixTierCurveMarker => "adventure-affix-tier-curve-marker.json",
            Store::CelestialShardFirstAwardMarker => "adventure-celestial-shard-first-award-marker.json",
            Store::CelestialShardIntoUniqueShardMarker => "adventure-celestial-shard-into-unique-shard-marker.json",
            Store::CraftTokenBackfillMarker => "adventure-craft-token-backfill-marker.json",
            Store::CraftTokenBackfillV2Marker => "adventure-craft-token-backfill-v2-marker.json",
            Store::CritFlagToAffixTrackingMarker => "adventure-crit-flag-to-affix-tracking-marker.json",
            Store::CritLineageBackfillMarker => "adventure-crit-lineage-backfill-marker.json",
            Store::CritReforgeEquippedBackfillMarker => "adventure-crit-reforge-equipped-backfill-marker.json",
            Store::CritValueNerfMarker => "adventure-crit-value-nerf-marker.json",
            Store::DuplicateUniqueEffectsCleanupMarker => "adventure-duplicate-unique-effects-cleanup-marker.json",
            Store::FightsStorageMigrationMarker => "adventure-fights-storage-migration-marker.json",
            Store::FlowlikewaterSwapMarker => "adventure-flowlikewater-swap-marker.json",
            Store::GlovesSpeedRebalanceMarker => "adventure-gloves-speed-rebalance-marker.json",
            Store::HelmRebalanceV2Marker => "adventure-helm-rebalance-v2-marker.json",
            Store::ItemAccuracyMarker => "adventure-item-accuracy-marker.json",
            Store::KibukahCompensationMarker => "adventure-kibukah-compensation-marker.json",
            Store::KrangleAccuracyMarker => "adventure-krangle-accuracy-marker.json",
            Store::LingeringEffectToEchoMarker => "adventure-lingering-effect-to-echo-marker.json",
            Store::PassiveKeyRenameMarker => "adventure-passive-key-rename-marker.json",
            Store::PityLaunchMarker => "adventure-pity-launch-marker.json",
            Store::PowerRollBackfillMarker => "adventure-power-roll-backfill-marker.json",
            Store::RefundRetiredDeadNodesMarker => "adventure-refund-retired-dead-nodes-marker.json",
            Store::StarterKitBackfillMarker => "adventure-starter-kit-backfill-marker.json",
            Store::UniqueShardFirstAwardMarker => "adventure-unique-shard-first-award-marker.json",
            Store::WingsGiveawayMarker => "adventure-wings-giveaway-marker.json",
            Store::WingsLaunchGrantMarker => "adventure-wings-launch-grant-marker.json",
            Store::Accounts => "adventure-accounts.json",
            Store::LiveTunables => "adventure-live-tunables.toml",
            Store::ItemBalance => "adventure-item-balance.toml",
            Store::PassiveOverrides => "adventure-passive-overrides.toml",
            Store::Bugreports => "adventure-bugreports.json",
            Store::PatchNotes => "patch-notes.json",
            Store::BotPublishedConstants => "bot-published-constants.json",
            Store::Templates => "templates",
            Store::Wiki => "wiki",
            Store::PublicAdventureOverlay => "public_adventure_overlay",
            Store::Logs => "logs",
        }
    }

    /// What a season reset does to it.
    pub const fn scope(self) -> StoreScope {
        match self {
            Store::Characters => StoreScope::World,
            Store::World => StoreScope::World,
            Store::Sessions => StoreScope::World,
            Store::ReforgeCooldown => StoreScope::World,
            Store::SpriteCount => StoreScope::World,
            Store::LastFights => StoreScope::World,
            Store::LastFightsJsonBak => StoreScope::World,
            Store::RampageState => StoreScope::World,
            Store::FightsBundleSeq => StoreScope::World,
            Store::FightsCoarseSeq => StoreScope::World,
            Store::FightsDetailSeq => StoreScope::World,
            Store::FightsSummarySeq => StoreScope::World,
            Store::FightsBundle => StoreScope::World,
            Store::FightsCoarse => StoreScope::World,
            Store::FightsDetail => StoreScope::World,
            Store::FightsSummary => StoreScope::World,
            Store::FightsPinned => StoreScope::World,
            Store::AffixTierCurveMarker => StoreScope::World,
            Store::CelestialShardFirstAwardMarker => StoreScope::World,
            Store::CelestialShardIntoUniqueShardMarker => StoreScope::World,
            Store::CraftTokenBackfillMarker => StoreScope::World,
            Store::CraftTokenBackfillV2Marker => StoreScope::World,
            Store::CritFlagToAffixTrackingMarker => StoreScope::World,
            Store::CritLineageBackfillMarker => StoreScope::World,
            Store::CritReforgeEquippedBackfillMarker => StoreScope::World,
            Store::CritValueNerfMarker => StoreScope::World,
            Store::DuplicateUniqueEffectsCleanupMarker => StoreScope::World,
            Store::FightsStorageMigrationMarker => StoreScope::World,
            Store::FlowlikewaterSwapMarker => StoreScope::World,
            Store::GlovesSpeedRebalanceMarker => StoreScope::World,
            Store::HelmRebalanceV2Marker => StoreScope::World,
            Store::ItemAccuracyMarker => StoreScope::World,
            Store::KibukahCompensationMarker => StoreScope::World,
            Store::KrangleAccuracyMarker => StoreScope::World,
            Store::LingeringEffectToEchoMarker => StoreScope::World,
            Store::PassiveKeyRenameMarker => StoreScope::World,
            Store::PityLaunchMarker => StoreScope::World,
            Store::PowerRollBackfillMarker => StoreScope::World,
            Store::RefundRetiredDeadNodesMarker => StoreScope::World,
            Store::StarterKitBackfillMarker => StoreScope::World,
            Store::UniqueShardFirstAwardMarker => StoreScope::World,
            Store::WingsGiveawayMarker => StoreScope::World,
            Store::WingsLaunchGrantMarker => StoreScope::World,
            Store::Accounts => StoreScope::Account,
            Store::LiveTunables => StoreScope::Config,
            Store::ItemBalance => StoreScope::Config,
            Store::PassiveOverrides => StoreScope::Config,
            Store::Bugreports => StoreScope::Config,
            Store::PatchNotes => StoreScope::Config,
            Store::BotPublishedConstants => StoreScope::Config,
            Store::Templates => StoreScope::NotAStore,
            Store::Wiki => StoreScope::NotAStore,
            Store::PublicAdventureOverlay => StoreScope::NotAStore,
            Store::Logs => StoreScope::NotAStore,
        }
    }

    /// File or directory - the reset command needs this, because
    /// removing a directory is a different call and a much larger
    /// mistake.
    pub const fn kind(self) -> StoreKind {
        match self {
            Store::Characters => StoreKind::File,
            Store::World => StoreKind::File,
            Store::Sessions => StoreKind::File,
            Store::ReforgeCooldown => StoreKind::File,
            Store::SpriteCount => StoreKind::File,
            Store::LastFights => StoreKind::File,
            Store::LastFightsJsonBak => StoreKind::File,
            Store::RampageState => StoreKind::File,
            Store::FightsBundleSeq => StoreKind::File,
            Store::FightsCoarseSeq => StoreKind::File,
            Store::FightsDetailSeq => StoreKind::File,
            Store::FightsSummarySeq => StoreKind::File,
            Store::FightsBundle => StoreKind::Dir,
            Store::FightsCoarse => StoreKind::Dir,
            Store::FightsDetail => StoreKind::Dir,
            Store::FightsSummary => StoreKind::Dir,
            Store::FightsPinned => StoreKind::Dir,
            Store::AffixTierCurveMarker => StoreKind::File,
            Store::CelestialShardFirstAwardMarker => StoreKind::File,
            Store::CelestialShardIntoUniqueShardMarker => StoreKind::File,
            Store::CraftTokenBackfillMarker => StoreKind::File,
            Store::CraftTokenBackfillV2Marker => StoreKind::File,
            Store::CritFlagToAffixTrackingMarker => StoreKind::File,
            Store::CritLineageBackfillMarker => StoreKind::File,
            Store::CritReforgeEquippedBackfillMarker => StoreKind::File,
            Store::CritValueNerfMarker => StoreKind::File,
            Store::DuplicateUniqueEffectsCleanupMarker => StoreKind::File,
            Store::FightsStorageMigrationMarker => StoreKind::File,
            Store::FlowlikewaterSwapMarker => StoreKind::File,
            Store::GlovesSpeedRebalanceMarker => StoreKind::File,
            Store::HelmRebalanceV2Marker => StoreKind::File,
            Store::ItemAccuracyMarker => StoreKind::File,
            Store::KibukahCompensationMarker => StoreKind::File,
            Store::KrangleAccuracyMarker => StoreKind::File,
            Store::LingeringEffectToEchoMarker => StoreKind::File,
            Store::PassiveKeyRenameMarker => StoreKind::File,
            Store::PityLaunchMarker => StoreKind::File,
            Store::PowerRollBackfillMarker => StoreKind::File,
            Store::RefundRetiredDeadNodesMarker => StoreKind::File,
            Store::StarterKitBackfillMarker => StoreKind::File,
            Store::UniqueShardFirstAwardMarker => StoreKind::File,
            Store::WingsGiveawayMarker => StoreKind::File,
            Store::WingsLaunchGrantMarker => StoreKind::File,
            Store::Accounts => StoreKind::File,
            Store::LiveTunables => StoreKind::File,
            Store::ItemBalance => StoreKind::File,
            Store::PassiveOverrides => StoreKind::File,
            Store::Bugreports => StoreKind::File,
            Store::PatchNotes => StoreKind::File,
            Store::BotPublishedConstants => StoreKind::File,
            Store::Templates => StoreKind::Dir,
            Store::Wiki => StoreKind::Dir,
            Store::PublicAdventureOverlay => StoreKind::Dir,
            Store::Logs => StoreKind::Dir,
        }
    }

    /// Why it is classified this way, in one line. Printed by the reset
    /// command before it deletes anything.
    pub const fn why(self) -> &'static str {
        match self {
            Store::Characters => "the roster - every character in this world",
            Store::World => "world state: stage, boss progress, the season's own position",
            Store::Sessions => "login sessions - invalidated at a reset by the documented cutover, so everyone logs in again against surviving accounts",
            Store::ReforgeCooldown => "per-character crafting cooldowns - meaningless once the characters are gone",
            Store::SpriteCount => "how many sprites existed last boot, so a grown roster grants a free model change to this world's characters",
            Store::LastFights => "RETIRED input to a completed marker-gated migration that split the old single-blob log into the four tier directories - absent because that migration consumed it, and still the only path that can upgrade a restored old-format backup",
            Store::LastFightsJsonBak => "the pre-split fight log, kept as a backup by the storage migration - last season's history in archived form, so it goes with the season",
            Store::RampageState => "rampage countdown, mirrored so a restart resumes it - absent because nothing has written it since `start_rampage` lost its last caller in the bot decoupling, not because nothing would",
            Store::FightsBundleSeq => "next sequence number for the bundle fight tier",
            Store::FightsCoarseSeq => "next sequence number for the coarse fight tier",
            Store::FightsDetailSeq => "next sequence number for the detail fight tier",
            Store::FightsSummarySeq => "next sequence number for the summary fight tier",
            Store::FightsBundle => "this world's fight history, bundle tier",
            Store::FightsCoarse => "this world's fight history, coarse tier",
            Store::FightsDetail => "this world's fight history, detail tier",
            Store::FightsSummary => "this world's fight history, summary tier",
            Store::FightsPinned => "fights a moderator deliberately pinned so pruning never touches them - THIS ONE IS A JUDGEMENT CALL: pinning means keep-forever against pruning, and a season reset is a different event from pruning. Classified World for consistency with the other fight tiers; say so if a pinned highlight should outlive its season",
            Store::AffixTierCurveMarker => "one-time migration marker - records that a backfill already ran against THIS world's characters",
            Store::CelestialShardFirstAwardMarker => "one-time migration marker - records that a backfill already ran against THIS world's characters",
            Store::CelestialShardIntoUniqueShardMarker => "one-time migration marker - records that a backfill already ran against THIS world's characters",
            Store::CraftTokenBackfillMarker => "one-time migration marker - records that a backfill already ran against THIS world's characters",
            Store::CraftTokenBackfillV2Marker => "one-time migration marker - records that a backfill already ran against THIS world's characters",
            Store::CritFlagToAffixTrackingMarker => "one-time migration marker - records that a backfill already ran against THIS world's characters",
            Store::CritLineageBackfillMarker => "one-time migration marker - records that a backfill already ran against THIS world's characters",
            Store::CritReforgeEquippedBackfillMarker => "one-time migration marker - records that a backfill already ran against THIS world's characters",
            Store::CritValueNerfMarker => "one-time migration marker - records that a backfill already ran against THIS world's characters",
            Store::DuplicateUniqueEffectsCleanupMarker => "one-time migration marker - records that a backfill already ran against THIS world's characters",
            Store::FightsStorageMigrationMarker => "one-time migration marker - records that a backfill already ran against THIS world's characters",
            Store::FlowlikewaterSwapMarker => "one-time migration marker - records that a backfill already ran against THIS world's characters",
            Store::GlovesSpeedRebalanceMarker => "one-time migration marker - records that a backfill already ran against THIS world's characters",
            Store::HelmRebalanceV2Marker => "one-time migration marker - records that a backfill already ran against THIS world's characters",
            Store::ItemAccuracyMarker => "one-time migration marker - records that a backfill already ran against THIS world's characters",
            Store::KibukahCompensationMarker => "one-time migration marker - records that a backfill already ran against THIS world's characters",
            Store::KrangleAccuracyMarker => "one-time migration marker - records that a backfill already ran against THIS world's characters",
            Store::LingeringEffectToEchoMarker => "one-time migration marker - records that a backfill already ran against THIS world's characters",
            Store::PassiveKeyRenameMarker => "one-time migration marker - records that a backfill already ran against THIS world's characters",
            Store::PityLaunchMarker => "one-time migration marker - records that a backfill already ran against THIS world's characters",
            Store::PowerRollBackfillMarker => "one-time migration marker - records that a backfill already ran against THIS world's characters",
            Store::RefundRetiredDeadNodesMarker => "one-time migration marker - records that a backfill already ran against THIS world's characters",
            Store::StarterKitBackfillMarker => "one-time migration marker - records that a backfill already ran against THIS world's characters",
            Store::UniqueShardFirstAwardMarker => "one-time migration marker - records that a backfill already ran against THIS world's characters",
            Store::WingsGiveawayMarker => "one-time migration marker - records that a backfill already ran against THIS world's characters",
            Store::WingsLaunchGrantMarker => "one-time migration marker - records that a backfill already ran against THIS world's characters",
            Store::Accounts => "logins and password hashes - who you are, as opposed to what you did last season. Survived previous resets only by NOT being named in the runbook; declared here so it survives by decision",
            Store::LiveTunables => "operator-set live tunables - configuration, not player state",
            Store::ItemBalance => "item balance table - configuration, not player state",
            Store::PassiveOverrides => "passive node overrides - configuration, not player state",
            Store::Bugreports => "player-submitted bug reports - about the game rather than about a character, and worth keeping across a season",
            Store::PatchNotes => "published patch notes - the game's own changelog, which outlives any one world",
            Store::BotPublishedConstants => "constants published for the bot to read - derived output, regenerated rather than owned by a world",
            Store::Templates => "shipped HTML templates - deployed assets, not persisted state",
            Store::Wiki => "live wiki content the OWNER edits by hand - a reset must never touch it",
            Store::PublicAdventureOverlay => "shipped overlay assets, including the custom sprite drop-in directory",
            Store::Logs => "process logs - operational output, not game state",
        }
    }
}

/// The store with this on-disk name, or `None` if it is not declared.
///
/// `None` is the interesting answer: it is what makes the reset refuse.
pub fn store_named(name: &str) -> Option<Store> {
    Store::ALL.iter().copied().find(|store| store.name() == name)
}

/// The classification for one entry name, or `None` if undeclared.
pub fn scope_of(name: &str) -> Option<StoreScope> {
    store_named(name).map(Store::scope)
}

/// The entries a season reset destroys.
pub fn world_scoped() -> impl Iterator<Item = Store> {
    Store::ALL.iter().copied().filter(|store| store.scope() == StoreScope::World)
}

/// Entries present in `dir` that the table does not declare, sorted.
///
/// **The one-directional reality check.** Anything here means the reset
/// cannot know whether to destroy it, so the reset must refuse rather
/// than guess. Never reports the reverse - a declared store that is
/// simply absent is the normal state of most of this table.
///
/// A directory that cannot be read yields an error rather than an empty
/// list: "I could not look" must not be mistaken for "there was nothing
/// there", which is precisely how a fail-closed guard turns into a
/// fail-open one.
pub fn undeclared_entries(dir: &Path) -> std::io::Result<Vec<String>> {
    let mut undeclared = BTreeSet::new();
    for entry in std::fs::read_dir(dir)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if store_named(&name).is_none() {
            undeclared.insert(name);
        }
    }
    Ok(undeclared.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The compile-time half of the bidirectional check: the table agrees
    /// with itself. No directory involved, so this holds on a fresh
    /// checkout, in CI, and on a machine that has never run the game.
    #[test]
    fn every_store_is_named_once_and_says_why() {
        assert!(!Store::ALL.is_empty(), "sanity: an empty table would make every other assertion here vacuous");

        let mut seen = BTreeSet::new();
        for store in Store::ALL {
            let spec = store.spec();
            assert!(!spec.name.is_empty(), "{store:?} has no name and cannot be matched against anything on disk");
            assert!(!spec.why.is_empty(), "{store:?} has no reason - the reset command PRINTS this before deleting, so a blank one leaves an operator approving a name with no explanation");
            assert!(seen.insert(spec.name), "{:?} duplicates the on-disk name {} - `store_named` would silently return whichever came first", store, spec.name);
        }
    }

    /// `ALL` is the ONE hand-maintained list here - `spec` is
    /// compiler-enforced, but Rust cannot enumerate an enum's variants
    /// without a derive macro, and adding one for this is not worth a
    /// dependency. So `ALL` is guarded two ways, and this test states the
    /// limit rather than implying more coverage than exists:
    ///
    /// * the LENGTH is asserted against the variant count, which catches a
    ///   variant added to the enum but forgotten here - the actual failure
    ///   mode. It is a hand-maintained number, and that is the honest
    ///   weak point of this file;
    /// * every entry round-trips through its own name, which catches a
    ///   name that does not match what `store_named` would find.
    ///
    /// Note what a bucket test cannot do for you: dropping `Accounts` from
    /// `ALL` happens to fail `all_four_buckets_are_populated` only because
    /// it is the sole Account-scoped store. Dropping any World-scoped
    /// entry would not, which is exactly why the length check is here.
    #[test]
    fn all_lists_every_variant_exactly_once() {
        assert_eq!(
            Store::ALL.len(),
            54,
            "ALL must list every variant of `Store` exactly once. If you added a store, add it here too and bump this number; if this fires without you touching the enum, something removed an entry."
        );
    }

    #[test]
    fn every_entry_in_all_round_trips_through_its_name() {
        for store in Store::ALL {
            assert_eq!(store_named(store.name()), Some(*store), "{store:?} is in ALL but `store_named` does not find it by its own name");
        }
    }

    /// Every bucket is populated. If one ever empties, that is a real
    /// change in what the game persists and should be noticed
    /// deliberately rather than by a silently-skipped loop.
    #[test]
    fn all_four_buckets_are_populated() {
        for scope in [StoreScope::World, StoreScope::Account, StoreScope::Config, StoreScope::NotAStore] {
            assert!(Store::ALL.iter().any(|store| store.scope() == scope), "{scope:?} has no entries - four buckets exist because the live directory needed four, so an empty one means the table has drifted from reality");
        }
    }

    /// The classifications this work exists to pin, by name, because each
    /// was a live decision rather than a default.
    #[test]
    fn the_decisions_that_were_actually_made_stay_made() {
        assert_eq!(Store::Accounts.scope(), StoreScope::Account, "accounts must survive a reset BY DECISION - they previously survived only by not appearing in the runbook");
        assert_eq!(Store::Characters.scope(), StoreScope::World, "the roster is what a season reset is for");
        assert_eq!(Store::Wiki.scope(), StoreScope::NotAStore, "the owner edits wiki content by hand - a reset touching it would destroy work no backup of the game covers");
        assert_eq!(Store::LiveTunables.scope(), StoreScope::Config, "operator configuration is not player state and does not belong to a world");

        // Markers world-scoped: a marker records what was applied to THIS
        // world's characters, so a fresh world must re-run the grant.
        assert_eq!(Store::StarterKitBackfillMarker.scope(), StoreScope::World, "a fresh world must re-run its starter-kit grant");

        // Declared despite being absent from live production - absence is
        // not deadness.
        assert_eq!(Store::LastFights.scope(), StoreScope::World, "absent because a completed migration consumed it - declared anyway");
        assert_eq!(Store::RampageState.scope(), StoreScope::World, "absent because nothing has written it - declared anyway");
    }

    /// An undeclared name has no store. This is the arm the reset guard
    /// stands on, so it is asserted directly rather than only through the
    /// guard.
    #[test]
    fn an_undeclared_name_has_no_store() {
        assert_eq!(store_named("adventure-something-nobody-classified.json"), None, "an unknown entry must return None - the reset refuses on exactly this");
        assert_eq!(scope_of(""), None);
    }

    /// `world_scoped` selects the destroy list and nothing else. A bug
    /// here deletes an account store, so it is checked by scope rather
    /// than by count.
    #[test]
    fn only_world_scoped_entries_are_selected_for_deletion() {
        assert!(world_scoped().all(|store| store.scope() == StoreScope::World), "the delete list must contain nothing but World-scoped entries");
        let names: Vec<&str> = world_scoped().map(Store::name).collect();
        assert!(names.contains(&Store::Characters.name()), "sanity: the roster must be in the delete list, or this test proves nothing");
        assert!(!names.contains(&Store::Accounts.name()), "accounts must NEVER be in the delete list");
        assert!(!names.contains(&Store::Wiki.name()), "wiki content must NEVER be in the delete list");
        assert!(!names.contains(&Store::LiveTunables.name()), "operator configuration must never be in the delete list");
    }

    /// `undeclared_entries` reports what is present and unclassified, and
    /// stays silent about what is merely absent. Asserted against a real
    /// directory, because the whole point is behaviour against a
    /// filesystem.
    #[test]
    fn undeclared_entries_reports_the_present_and_ignores_the_absent() {
        let dir = std::env::temp_dir().join(format!("pod-stores-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir must be creatable");

        std::fs::write(dir.join(Store::Characters.name()), "{}").unwrap();
        std::fs::create_dir_all(dir.join(Store::Wiki.name())).unwrap();
        assert!(
            undeclared_entries(&dir).unwrap().is_empty(),
            "declared entries must not be reported - and the other 50-odd declared stores are ABSENT from this directory, which must also not be reported"
        );

        std::fs::write(dir.join("adventure-mystery.json"), "{}").unwrap();
        assert_eq!(undeclared_entries(&dir).unwrap(), vec!["adventure-mystery.json".to_string()], "an unclassified entry must be reported by name");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Failing to read the directory is an error, not an empty list. "I
    /// could not look" must never be mistaken for "there was nothing
    /// there" - that is the exact step that turns a fail-closed guard
    /// into a fail-open one.
    #[test]
    fn an_unreadable_directory_is_an_error_not_an_empty_answer() {
        let missing = std::env::temp_dir().join("pod-stores-definitely-does-not-exist-9e3f1a");
        let _ = std::fs::remove_dir_all(&missing);
        assert!(undeclared_entries(&missing).is_err(), "a directory that cannot be read must surface as an error - an empty Vec here would read as 'nothing undeclared' and let a reset proceed blind");
    }
}

/// Prints the on-disk name, so a log line naming a store reads the same
/// as it did when these were bare string literals.
impl std::fmt::Display for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}
