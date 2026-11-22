//! The L1 case registry: every independently executed L1 case of each
//! category, its family, and whether the category's routine sweep runs it.
//!
//! A **case** is one declared workload, fixture and load configuration; a
//! **sweep** is one category invocation over selected cases. The routine
//! selection, `--list-cases`, `--cases` validation and the order a default
//! sweep runs in all read [`Category::cases`], so the three cannot disagree.
//! A selector is the case id, or `family/id` where the id recurs within its
//! category; `family/id` is always accepted. Resolution refuses unknown,
//! ambiguous or repeated selectors before any build artifact is read or case
//! launched.

use std::fmt::Write as _;

use wyrd_testing::experiment::Engine;
use wyrd_testing::profile::ProfileMode;

use crate::flags;

/// The three L1 categories, each one `mise run benchmark:l1:<category>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// Direct verification through the public SDKs.
    Verification,
    /// The AI gateway's added overhead.
    Gateway,
    /// Bifrost ingest and reads.
    Bifrost,
}

/// One registered L1 case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Case {
    /// The category whose command runs it.
    pub category: Category,
    /// The family: the experiment that declares the case's step.
    pub family: &'static str,
    /// The step id within its family.
    pub id: &'static str,
    /// Whether the category's routine sweep runs it; a targeted case runs
    /// only when selected.
    pub routine: bool,
}

/// The Verification family every case belongs to.
const DIRECT_VERIFY: &str = "direct-verify";

/// The Gateway family every case belongs to.
const OVERHEAD: &str = "overhead";

/// Verification cases in registry order: the routine five first, in sweep
/// order, then the targeted SDK closed-loop, Rust concurrency and Agent-only
/// model cases.
const VERIFICATION: [(&str, &str, bool); 14] = [
    (DIRECT_VERIFY, "direct-verify-c1", true),
    (DIRECT_VERIFY, "direct-verify-c64", true),
    (DIRECT_VERIFY, "direct-verify-r3000", true),
    (DIRECT_VERIFY, "direct-verify-python-r3000", true),
    (DIRECT_VERIFY, "direct-verify-typescript-r3000", true),
    (DIRECT_VERIFY, "direct-verify-python-c1", false),
    (DIRECT_VERIFY, "direct-verify-python-c64", false),
    (DIRECT_VERIFY, "direct-verify-typescript-c1", false),
    (DIRECT_VERIFY, "direct-verify-typescript-c64", false),
    (DIRECT_VERIFY, "direct-verify-c4", false),
    (DIRECT_VERIFY, "direct-verify-c16", false),
    (DIRECT_VERIFY, "direct-verify-c32", false),
    (DIRECT_VERIFY, "direct-verify-model-c1", false),
    (DIRECT_VERIFY, "direct-verify-model-c64", false),
];

/// Gateway cases in registry order: the routine five, then the three
/// targeted single-client controls.
const GATEWAY: [(&str, &str, bool); 8] = [
    (OVERHEAD, "buffered-1kib-capture-metadata.c1", true),
    (OVERHEAD, "buffered-1kib-capture-metadata.c64", true),
    (OVERHEAD, "streamed-1kib-capture-metadata.c64", true),
    (OVERHEAD, "buffered-64kib-capture-metadata.c64", true),
    (OVERHEAD, "buffered-64kib-capture-payload.c64", true),
    (OVERHEAD, "streamed-1kib-capture-metadata.c1", false),
    (OVERHEAD, "buffered-64kib-capture-metadata.c1", false),
    (OVERHEAD, "buffered-64kib-capture-payload.c1", false),
];

/// Bifrost cases in registry order: the routine eighteen in sweep order
/// (writes, hot, Parquet, UI SQL), then the targeted read shapes, the
/// single-client UI arms, and the 10x-volume and learned-layout UI families.
const BIFROST: [(&str, &str, bool); 43] = [
    ("writes-parquet", "otlp-spans-c1", true),
    ("writes-parquet", "otlp-spans-c64", true),
    ("writes-parquet", "otlp-spans-1k-c1", true),
    ("writes-parquet", "otlp-spans-1k-c64", true),
    ("writes-parquet", "otlp-logs-c1", true),
    ("writes-parquet", "otlp-logs-c64", true),
    ("writes-parquet", "native-write-c1", true),
    ("writes-parquet", "native-write-c64", true),
    ("hot", "point-memtable-c1", true),
    ("hot", "point-memtable-c64", true),
    ("hot", "point-staged-c1", true),
    ("parquet", "point-c1", true),
    ("parquet", "point-c64", true),
    ("parquet", "full-scan-c1", true),
    ("parquet", "span-trace-c64", true),
    ("ui-sql", "memtable-ui-c8", true),
    ("ui-sql", "staged-ui-c8", true),
    ("ui-sql", "published-ui-c8", true),
    ("hot", "small-aggregate-memtable-c1", false),
    ("hot", "small-aggregate-staged-c1", false),
    ("hot", "span-shredded-memtable-c1", false),
    ("hot", "span-shredded-staged-c1", false),
    ("hot", "span-long-tail-memtable-c1", false),
    ("hot", "span-long-tail-staged-c1", false),
    ("parquet", "small-aggregate-c1", false),
    ("parquet", "span-trace-c1", false),
    ("parquet", "span-shredded-c1", false),
    ("parquet", "span-long-tail-c1", false),
    ("ui-sql", "memtable-ui-c1", false),
    ("ui-sql", "staged-ui-c1", false),
    ("ui-sql", "published-ui-c1", false),
    ("ui-sql-10x", "memtable-ui-c1", false),
    ("ui-sql-10x", "memtable-ui-c8", false),
    ("ui-sql-10x", "staged-ui-c1", false),
    ("ui-sql-10x", "staged-ui-c8", false),
    ("ui-sql-10x", "published-ui-c1", false),
    ("ui-sql-10x", "published-ui-c8", false),
    ("ui-sql-learned", "memtable-ui-c1", false),
    ("ui-sql-learned", "memtable-ui-c8", false),
    ("ui-sql-learned", "staged-ui-c1", false),
    ("ui-sql-learned", "staged-ui-c8", false),
    ("ui-sql-learned", "published-ui-c1", false),
    ("ui-sql-learned", "published-ui-c8", false),
];

/// Flags a category command accepts beside `--cases` and `--profile`.
const SWITCHES: [&str; 2] = ["--list-cases", "--allow-dirty"];

impl Category {
    /// Every category, in documentation order.
    pub const ALL: [Self; 3] = [Self::Verification, Self::Gateway, Self::Bifrost];

    /// The category's command word, as in `benchmark:l1:<name>`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Verification => "verification",
            Self::Gateway => "gateway",
            Self::Bifrost => "bifrost",
        }
    }

    /// The category `name` selects.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|category| category.name() == name)
    }

    /// The engine whose workloads run the category's cases.
    #[must_use]
    pub const fn engine(self) -> Engine {
        match self {
            Self::Verification => Engine::Verification,
            Self::Gateway => Engine::Gateway,
            Self::Bifrost => Engine::Bifrost,
        }
    }

    /// Valued flags the category forwards to each case: Gateway's
    /// `--pace <requests/s>` offers each window open loop at that rate.
    #[must_use]
    pub const fn valued(self) -> &'static [&'static str] {
        match self {
            Self::Gateway => &["--pace"],
            Self::Verification | Self::Bifrost => &[],
        }
    }

    /// Every registered case of the category, in registry order: routine
    /// cases first, in the order the routine sweep runs them.
    #[must_use]
    pub fn cases(self) -> Vec<Case> {
        let table: &[(&str, &str, bool)] = match self {
            Self::Verification => &VERIFICATION,
            Self::Gateway => &GATEWAY,
            Self::Bifrost => &BIFROST,
        };
        table
            .iter()
            .map(|(family, id, routine)| Case {
                category: self,
                family,
                id,
                routine: *routine,
            })
            .collect()
    }

    /// The routine sweep, in declared order.
    #[must_use]
    pub fn routine(self) -> Vec<Case> {
        self.cases()
            .into_iter()
            .filter(|case| case.routine)
            .collect()
    }

    /// The `--list-cases` output: one line per case with its exact
    /// selector, family and routine or targeted status, in registry order.
    #[must_use]
    pub fn list(self) -> String {
        let mut out = format!(
            "{} L1 cases ({} routine, {} targeted)\n",
            self.name(),
            self.routine().len(),
            self.cases().len() - self.routine().len()
        );
        for case in self.cases() {
            let status = if case.routine { "routine" } else { "targeted" };
            let _ = writeln!(out, "{}\t{}\t{status}", case.selector(), case.family);
        }
        out
    }

    /// The case `selector` names: `family/id`, or a bare id that names
    /// exactly one case of the category.
    ///
    /// # Errors
    ///
    /// Returns a message naming an unknown selector, or every
    /// family-qualified candidate of an ambiguous one.
    pub fn case(self, selector: &str) -> Result<Case, String> {
        let cases = self.cases();
        if let Some((family, id)) = selector.split_once('/') {
            return cases
                .into_iter()
                .find(|case| case.family == family && case.id == id)
                .ok_or_else(|| {
                    format!(
                        "`{selector}` is not a {} case; run with --list-cases",
                        self.name()
                    )
                });
        }
        let matching: Vec<Case> = cases
            .into_iter()
            .filter(|case| case.id == selector)
            .collect();
        match matching.as_slice() {
            [case] => Ok(*case),
            [] => Err(format!(
                "`{selector}` is not a {} case; run with --list-cases",
                self.name()
            )),
            several => Err(format!(
                "`{selector}` is ambiguous; name one of {}",
                several
                    .iter()
                    .map(|case| format!("`{}/{}`", case.family, case.id))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }
}

impl Case {
    /// Whether another case of the category shares this id, so its
    /// selector needs its family.
    fn recurs(&self) -> bool {
        self.category
            .cases()
            .iter()
            .filter(|case| case.id == self.id)
            .count()
            > 1
    }

    /// The case's exact selector: its id, or `family/id` when the id
    /// recurs within the category.
    #[must_use]
    pub fn selector(&self) -> String {
        if self.recurs() {
            format!("{}/{}", self.family, self.id)
        } else {
            self.id.to_owned()
        }
    }

    /// The command line, after the program name, of the engine plan that
    /// declares exactly this case: `l1 <engine> [family] --steps <id>`
    /// with the forwarded `flags`. Every selection of the case produces the
    /// same plan arguments.
    #[must_use]
    pub fn plan_arguments(&self, flags: &[String]) -> Vec<String> {
        let mut arguments = vec!["capacity".to_owned()];
        if self.category == Category::Bifrost {
            arguments.push(self.family.to_owned());
        }
        arguments.extend(["--steps".to_owned(), self.id.to_owned()]);
        arguments.extend(flags.iter().cloned());
        arguments
    }
}

/// What a category command asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// `--list-cases`: print [`Category::list`] and run nothing.
    List,
    /// Run `cases` in order.
    Sweep(Selection),
}

/// A resolved sweep.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// The category.
    pub category: Category,
    /// The selected cases, in the order they run.
    pub cases: Vec<Case>,
    /// Whether `--cases` selected them; otherwise the routine sweep.
    pub explicit: bool,
    /// The `--profile` mode, when profiling.
    pub profile: Option<ProfileMode>,
    /// Flags every case receives unchanged, such as `--allow-dirty` and
    /// Gateway's `--pace <rate>`.
    pub forwarded: Vec<String>,
}

/// Reads a category command line (program name first, then the flags
/// after the category): `--list-cases`, or a sweep of the routine cases or
/// of exactly the `--cases` selection, in the requested order.
///
/// # Errors
///
/// Returns a message for an unknown flag, an invalid `--profile`, a
/// `--list-cases` combined with a selection, or an unknown, ambiguous,
/// empty or repeated selector; nothing is built or launched.
pub fn resolve(category: Category, arguments: &[String]) -> Result<Request, String> {
    let valued: Vec<&str> = ["--cases"]
        .iter()
        .chain(category.valued())
        .copied()
        .collect();
    if arguments.iter().any(|argument| argument == "--smoke") {
        return Err(
            "unknown argument `--smoke`: the L1 smoke runs in the harness test lane".to_owned(),
        );
    }
    crate::levels::known(arguments, &valued, &SWITCHES)?;
    let profile = crate::levels::profile(arguments)?;
    let listing = arguments.iter().any(|argument| argument == "--list-cases");
    let selected = flags::list(
        arguments,
        "--cases",
        "case selectors from --list-cases",
        |selector| category.case(selector).ok(),
    )
    .map_err(|message| {
        flags::value(arguments, "--cases")
            .ok()
            .flatten()
            .and_then(|list| {
                list.split(',')
                    .find_map(|selector| category.case(selector.trim()).err())
            })
            .unwrap_or(message)
    })?;
    if listing {
        if selected.is_some() || profile.is_some() {
            return Err("--list-cases runs nothing and takes no other flag".to_owned());
        }
        return Ok(Request::List);
    }
    let mut forwarded = Vec::new();
    if arguments.iter().any(|argument| argument == "--allow-dirty") {
        forwarded.push("--allow-dirty".to_owned());
    }
    for flag in category.valued() {
        if let Some(value) = flags::value(arguments, flag)? {
            forwarded.extend([(*flag).to_owned(), value.to_owned()]);
        }
    }
    let explicit = selected.is_some();
    Ok(Request::Sweep(Selection {
        category,
        cases: selected.unwrap_or_else(|| category.routine()),
        explicit,
        profile,
        forwarded,
    }))
}

#[cfg(test)]
mod tests {
    use super::{Category, Request, Selection, resolve};
    use wyrd_testing::profile::ProfileMode;

    /// A category command line of `rest` after the program name.
    fn line(rest: &[&str]) -> Vec<String> {
        std::iter::once("capacity")
            .chain(rest.iter().copied())
            .map(str::to_owned)
            .collect()
    }

    /// The sweep `rest` resolves to.
    ///
    /// # Panics
    ///
    /// Panics when `rest` does not resolve to a sweep.
    fn sweep(category: Category, rest: &[&str]) -> Selection {
        match resolve(category, &line(rest)) {
            Ok(Request::Sweep(selection)) => selection,
            other => panic!("expected a sweep, got {other:?}"),
        }
    }

    /// The selectors of a sweep, in run order.
    fn selectors(selection: &Selection) -> Vec<String> {
        selection.cases.iter().map(super::Case::selector).collect()
    }

    /// Each category's default sweep is exactly the audit's routine
    /// selection in its declared order, and the registry holds the audit's
    /// full inventory.
    #[test]
    fn categories_resolve_their_routine_sweeps() {
        let verification = sweep(Category::Verification, &[]);
        assert!(!verification.explicit);
        assert_eq!(
            selectors(&verification),
            [
                "direct-verify-c1",
                "direct-verify-c64",
                "direct-verify-r3000",
                "direct-verify-python-r3000",
                "direct-verify-typescript-r3000",
            ]
        );
        assert_eq!(
            selectors(&sweep(Category::Gateway, &[])),
            [
                "buffered-1kib-capture-metadata.c1",
                "buffered-1kib-capture-metadata.c64",
                "streamed-1kib-capture-metadata.c64",
                "buffered-64kib-capture-metadata.c64",
                "buffered-64kib-capture-payload.c64",
            ]
        );
        assert_eq!(
            selectors(&sweep(Category::Bifrost, &[])),
            [
                "otlp-spans-c1",
                "otlp-spans-c64",
                "otlp-spans-1k-c1",
                "otlp-spans-1k-c64",
                "otlp-logs-c1",
                "otlp-logs-c64",
                "native-write-c1",
                "native-write-c64",
                "point-memtable-c1",
                "point-memtable-c64",
                "point-staged-c1",
                "point-c1",
                "point-c64",
                "full-scan-c1",
                "span-trace-c64",
                "ui-sql/memtable-ui-c8",
                "ui-sql/staged-ui-c8",
                "ui-sql/published-ui-c8",
            ]
        );
        let counts: Vec<(usize, usize)> = Category::ALL
            .map(|category| (category.routine().len(), category.cases().len()))
            .to_vec();
        assert_eq!(counts, [(5, 14), (5, 8), (18, 43)]);
    }

    /// `--list-cases` names every exact selector once with its routine or
    /// targeted status, family-qualifying recurring Bifrost ids.
    #[test]
    fn listing_names_every_selector_and_status() {
        assert_eq!(
            resolve(Category::Bifrost, &line(&["--list-cases"])),
            Ok(Request::List)
        );
        let listed = Category::Bifrost.list();
        assert!(listed.starts_with("bifrost L1 cases (18 routine, 25 targeted)\n"));
        assert!(listed.contains("ui-sql/memtable-ui-c8\tui-sql\troutine\n"));
        assert!(listed.contains("ui-sql-10x/memtable-ui-c8\tui-sql-10x\ttargeted\n"));
        assert!(listed.contains("ui-sql-learned/published-ui-c1\tui-sql-learned\ttargeted\n"));
        assert!(listed.contains("small-aggregate-staged-c1\thot\ttargeted\n"));
        assert!(listed.contains("point-c1\tparquet\troutine\n"));
        for category in Category::ALL {
            let listed = category.list();
            for case in category.cases() {
                assert_eq!(
                    listed
                        .lines()
                        .filter(|line| line.split('\t').next() == Some(case.selector().as_str()))
                        .count(),
                    1,
                    "{}",
                    case.selector()
                );
            }
        }
        assert!(
            Category::Verification
                .list()
                .contains("direct-verify-c16\tdirect-verify\ttargeted\n")
        );
        assert!(
            resolve(
                Category::Bifrost,
                &line(&["--list-cases", "--cases", "point-c1"])
            )
            .is_err()
        );
    }

    /// Explicit selection runs exactly the named cases in the given order,
    /// targeted ones included, and accepts family-qualified selectors.
    #[test]
    fn explicit_selection_keeps_request_order() {
        let selection = sweep(
            Category::Bifrost,
            &[
                "--cases",
                "ui-sql-10x/staged-ui-c1,point-c64,hot/point-staged-c1",
            ],
        );
        assert!(selection.explicit);
        assert_eq!(
            selectors(&selection),
            ["ui-sql-10x/staged-ui-c1", "point-c64", "point-staged-c1"]
        );
        let verification = sweep(
            Category::Verification,
            &[
                "--cases",
                "direct-verify-c16,direct-verify-model-c64,direct-verify-c1",
            ],
        );
        assert_eq!(
            selectors(&verification),
            [
                "direct-verify-c16",
                "direct-verify-model-c64",
                "direct-verify-c1"
            ]
        );
    }

    /// Unknown, ambiguous, repeated and empty selectors, unknown flags and
    /// a bad profile mode fail resolution, naming the problem.
    #[test]
    fn bad_selectors_fail_before_launch() {
        let refused = |category: Category, rest: &[&str]| {
            resolve(category, &line(rest)).expect_err("refused")
        };
        assert!(refused(Category::Bifrost, &["--cases", "memtable-ui-c8"]).contains("ambiguous"));
        assert!(
            refused(Category::Bifrost, &["--cases", "memtable-ui-c8"])
                .contains("`ui-sql-learned/memtable-ui-c8`")
        );
        assert!(
            refused(Category::Gateway, &["--cases", "point-c1"]).contains("not a gateway case")
        );
        assert!(
            refused(
                Category::Verification,
                &["--cases", "direct-verify-c1,direct-verify-c1"]
            )
            .contains("twice")
        );
        assert!(refused(Category::Verification, &["--cases"]).contains("needs a value"));
        assert!(
            refused(Category::Verification, &["--cases", "direct-verify-c1,"])
                .contains("not a verification case")
        );
        assert!(
            refused(Category::Verification, &["--steps", "direct-verify-c1"])
                .contains("unknown argument")
        );
        assert!(refused(Category::Verification, &["--smoke"]).contains("unknown argument"));
        assert!(
            refused(Category::Verification, &["--profile", "wall"]).contains("cpu, heap, or both")
        );
        assert!(refused(Category::Verification, &["--pace", "10"]).contains("unknown argument"));
    }

    /// Every profile option applies through the same selection, with and
    /// without `--cases`, and only changes the mode.
    #[test]
    fn profiles_apply_with_and_without_selection() {
        for (flag, mode) in [
            (Some("cpu"), ProfileMode::Cpu),
            (Some("heap"), ProfileMode::Heap),
            (Some("both"), ProfileMode::Both),
            (None, ProfileMode::Both),
        ] {
            let mut rest = vec!["--profile"];
            rest.extend(flag);
            let routine = sweep(Category::Gateway, &rest);
            assert_eq!(routine.profile, Some(mode));
            assert_eq!(routine.cases, Category::Gateway.routine());
            rest.extend(["--cases", "buffered-64kib-capture-payload.c1"]);
            let targeted = sweep(Category::Gateway, &rest);
            assert_eq!(targeted.profile, Some(mode));
            assert_eq!(selectors(&targeted), ["buffered-64kib-capture-payload.c1"]);
        }
        let paced = sweep(Category::Gateway, &["--pace", "4000", "--allow-dirty"]);
        assert_eq!(paced.forwarded, ["--allow-dirty", "--pace", "4000"]);
    }

    /// A case's plan arguments are the same whether it is selected alone,
    /// in a subset, reordered, or by the routine sweep.
    #[test]
    fn a_case_plans_identically_in_every_selection() {
        let plans = |rest: &[&str]| {
            sweep(Category::Bifrost, rest)
                .cases
                .into_iter()
                .map(|case| (case.selector(), case.plan_arguments(&[])))
                .collect::<Vec<_>>()
        };
        let alone = plans(&["--cases", "otlp-logs-c64"]);
        let subset = plans(&["--cases", "otlp-logs-c1,otlp-logs-c64"]);
        let reversed = plans(&["--cases", "otlp-logs-c64,otlp-logs-c1"]);
        let routine = plans(&[]);
        let of = |plans: &[(String, Vec<String>)]| {
            plans
                .iter()
                .find(|(selector, _)| selector == "otlp-logs-c64")
                .map(|(_, arguments)| arguments.clone())
                .expect("selected")
        };
        assert_eq!(
            of(&alone),
            ["capacity", "writes-parquet", "--steps", "otlp-logs-c64"]
        );
        for other in [&subset, &reversed, &routine] {
            assert_eq!(of(other), of(&alone));
        }
    }
}
