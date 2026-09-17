// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `SearchSession` behaviour (contracts/library-and-search-core.md §1/§6,
//! data-model.md §2.1, FR-001/002/015/018): debounce timing, trim/empty
//! reset, stale-generation discard, fixed group order + omission,
//! show-more offsets, rate-limit-keeps-stale-and-refreshing, and the
//! offline gate.

use std::time::{Duration, Instant};

use modplayer_audio_source::{
    Availability, CatalogError, SearchGroupPage, SearchHit, SearchKind, SearchPage, SourceCommand,
    TrackId, TrackRef,
};
use modplayer_core::{GroupState, SearchSession};

fn track_hit(id: &str) -> SearchHit {
    SearchHit::Track(TrackRef::new(
        TrackId::new(format!("spotify:track:{id}")).unwrap(),
        format!("Title {id}"),
        vec!["Artist".to_string()],
        None,
        None,
        180_000,
        Availability::Available,
    ))
}

/// A successful reply answering every group with `tracks_count` track hits
/// and the other three empty — the common single-request-id shape a
/// `SearchSession::tick` command's reply takes.
fn full_page(tracks_count: usize) -> SearchPage {
    SearchPage {
        groups: vec![
            SearchGroupPage {
                kind: SearchKind::Track,
                items: (0..tracks_count)
                    .map(|i| track_hit(&i.to_string()))
                    .collect(),
                next_offset: None,
            },
            SearchGroupPage {
                kind: SearchKind::Album,
                items: vec![],
                next_offset: None,
            },
            SearchGroupPage {
                kind: SearchKind::Artist,
                items: vec![],
                next_offset: None,
            },
            SearchGroupPage {
                kind: SearchKind::Playlist,
                items: vec![],
                next_offset: None,
            },
        ],
        unsupported: vec![],
    }
}

/// Extract the lone `SearchCatalog` command a `tick`/`show_more` call
/// produced, panicking if there wasn't exactly one.
fn expect_one_search_command(cmds: Vec<SourceCommand>) -> SourceCommand {
    assert_eq!(cmds.len(), 1, "expected exactly one command, got {cmds:?}");
    cmds.into_iter().next().unwrap()
}

// -- Debounce ----------------------------------------------------------

#[test]
fn edit_before_149ms_issues_no_request() {
    let mut session = SearchSession::new();
    let t0 = Instant::now();
    session.set_query("abba", t0);
    let cmds = session.tick(t0 + Duration::from_millis(149));
    assert!(cmds.is_empty(), "149 ms must not fire a request yet");
    assert_eq!(session.group(SearchKind::Track), &GroupState::Idle);
}

#[test]
fn edit_at_150ms_issues_exactly_one_request() {
    let mut session = SearchSession::new();
    let t0 = Instant::now();
    session.set_query("abba", t0);
    let cmds = session.tick(t0 + Duration::from_millis(150));
    let cmd = expect_one_search_command(cmds);
    match cmd {
        SourceCommand::SearchCatalog {
            query,
            kinds,
            offset,
            limit,
            ..
        } => {
            assert_eq!(query, "abba");
            assert_eq!(
                kinds,
                vec![
                    SearchKind::Track,
                    SearchKind::Album,
                    SearchKind::Artist,
                    SearchKind::Playlist
                ]
            );
            assert_eq!(offset, 0);
            assert_eq!(limit, 20);
        }
        other => panic!("expected SearchCatalog, got {other:?}"),
    }
    for (_, group) in session.groups() {
        assert_eq!(group, &GroupState::Pending);
    }
}

// -- Trim / empty --------------------------------------------------------

#[test]
fn trimmed_empty_query_issues_no_request_and_resets_to_idle() {
    let mut session = SearchSession::new();
    let t0 = Instant::now();
    session.set_query("abba", t0);
    session.set_query("   ", t0 + Duration::from_millis(10));
    let cmds = session.tick(t0 + Duration::from_millis(200));
    assert!(
        cmds.is_empty(),
        "an empty (trimmed) query must issue nothing"
    );
    for (_, group) in session.groups() {
        assert_eq!(group, &GroupState::Idle);
    }
    assert_eq!(session.query(), "");
}

// -- Stale-generation discard --------------------------------------------

#[test]
fn a_reply_for_an_older_generation_is_discarded() {
    let mut session = SearchSession::new();
    let t0 = Instant::now();
    session.set_query("abba", t0);
    let cmd = expect_one_search_command(session.tick(t0 + Duration::from_millis(150)));
    let SourceCommand::SearchCatalog { request_id, .. } = cmd else {
        unreachable!()
    };

    // A fresh edit + a second debounced tick bumps the generation before
    // the first reply ever arrives.
    session.set_query("queen", t0 + Duration::from_millis(200));
    let _ = expect_one_search_command(session.tick(t0 + Duration::from_millis(400)));

    // The stale reply must not touch the (now second-generation) state.
    session.apply_reply(request_id, Ok(full_page(3)));
    assert_eq!(session.group(SearchKind::Track), &GroupState::Pending);
}

// -- Group order + omission ----------------------------------------------

#[test]
fn groups_iterate_in_the_fixed_order_regardless_of_reply_order() {
    let mut session = SearchSession::new();
    let t0 = Instant::now();
    session.set_query("abba", t0);
    let cmd = expect_one_search_command(session.tick(t0 + Duration::from_millis(150)));
    let SourceCommand::SearchCatalog { request_id, .. } = cmd else {
        unreachable!()
    };

    let page = SearchPage {
        groups: vec![
            // Deliberately out of order in the wire reply.
            SearchGroupPage {
                kind: SearchKind::Playlist,
                items: vec![],
                next_offset: None,
            },
            SearchGroupPage {
                kind: SearchKind::Track,
                items: vec![track_hit("a")],
                next_offset: None,
            },
        ],
        unsupported: vec![SearchKind::Artist],
    };
    session.apply_reply(request_id, Ok(page));

    let order: Vec<SearchKind> = session.groups().map(|(kind, _)| kind).collect();
    assert_eq!(
        order,
        vec![
            SearchKind::Track,
            SearchKind::Album,
            SearchKind::Artist,
            SearchKind::Playlist
        ]
    );
    assert!(matches!(
        session.group(SearchKind::Track),
        GroupState::Loaded { .. }
    ));
    // Albums group answered nothing and wasn't named `unsupported` -> Empty
    // (omitted by the view), never left hanging as `Pending`.
    assert_eq!(session.group(SearchKind::Album), &GroupState::Empty);
    assert_eq!(session.group(SearchKind::Artist), &GroupState::Unsupported);
    assert_eq!(session.group(SearchKind::Playlist), &GroupState::Empty);
}

#[test]
fn all_four_groups_empty_is_the_no_results_state() {
    let mut session = SearchSession::new();
    let t0 = Instant::now();
    session.set_query("zzzzz", t0);
    let cmd = expect_one_search_command(session.tick(t0 + Duration::from_millis(150)));
    let SourceCommand::SearchCatalog { request_id, .. } = cmd else {
        unreachable!()
    };
    session.apply_reply(request_id, Ok(full_page(0)));
    assert!(session.is_no_results());
}

// -- Show-more offsets ----------------------------------------------------

#[test]
fn show_more_requests_the_next_offset_and_appends_on_reply() {
    let mut session = SearchSession::new();
    let t0 = Instant::now();
    session.set_query("abba", t0);
    let cmd = expect_one_search_command(session.tick(t0 + Duration::from_millis(150)));
    let SourceCommand::SearchCatalog { request_id, .. } = cmd else {
        unreachable!()
    };
    let mut page = full_page(20);
    page.groups[0].next_offset = Some(20);
    session.apply_reply(request_id, Ok(page));

    let cmd = session
        .show_more(SearchKind::Track)
        .expect("a further page exists");
    let SourceCommand::SearchCatalog {
        request_id: page_request_id,
        offset,
        kinds,
        limit,
        ..
    } = cmd
    else {
        unreachable!()
    };
    assert_eq!(offset, 20, "show more must ask for items.len()");
    assert_eq!(kinds, vec![SearchKind::Track]);
    assert_eq!(limit, 20);

    // A second show-more before the first replies is a no-op.
    assert!(session.show_more(SearchKind::Track).is_none());

    let mut second_page = SearchPage {
        groups: vec![SearchGroupPage {
            kind: SearchKind::Track,
            items: (20..40).map(|i| track_hit(&i.to_string())).collect(),
            next_offset: None,
        }],
        unsupported: vec![],
    };
    second_page.groups[0].next_offset = None;
    session.apply_reply(page_request_id, Ok(second_page));

    let GroupState::Loaded {
        items,
        next_offset,
        loading_more,
    } = session.group(SearchKind::Track)
    else {
        panic!("expected Loaded after show-more reply");
    };
    assert_eq!(items.len(), 40, "the new page's items must be appended");
    assert_eq!(*next_offset, None);
    assert!(!loading_more);
}

// -- Rate limit keeps stale + refreshing ----------------------------------

#[test]
fn rate_limited_initial_reply_marks_every_pending_group_refreshing() {
    let mut session = SearchSession::new();
    let t0 = Instant::now();
    session.set_query("abba", t0);
    let cmd = expect_one_search_command(session.tick(t0 + Duration::from_millis(150)));
    let SourceCommand::SearchCatalog { request_id, .. } = cmd else {
        unreachable!()
    };

    session.apply_reply(
        request_id,
        Err(CatalogError::RateLimited {
            retry_after_ms: Some(2_000),
        }),
    );

    assert!(session.refreshing());
    assert_eq!(
        session.group(SearchKind::Track),
        &GroupState::RateLimited { stale: None }
    );
}

#[test]
fn rate_limited_show_more_reply_keeps_the_previous_page_as_stale() {
    let mut session = SearchSession::new();
    let t0 = Instant::now();
    session.set_query("abba", t0);
    let cmd = expect_one_search_command(session.tick(t0 + Duration::from_millis(150)));
    let SourceCommand::SearchCatalog { request_id, .. } = cmd else {
        unreachable!()
    };
    let mut page = full_page(5);
    page.groups[0].next_offset = Some(20);
    session.apply_reply(request_id, Ok(page));

    let cmd = session
        .show_more(SearchKind::Track)
        .expect("a further page exists");
    let SourceCommand::SearchCatalog {
        request_id: page_request_id,
        ..
    } = cmd
    else {
        unreachable!()
    };

    session.apply_reply(
        page_request_id,
        Err(CatalogError::RateLimited {
            retry_after_ms: None,
        }),
    );

    assert!(session.refreshing());
    let GroupState::RateLimited { stale } = session.group(SearchKind::Track) else {
        panic!("expected RateLimited after a show-more rate limit");
    };
    assert_eq!(
        stale.as_ref().map(Vec::len),
        Some(5),
        "the previous page's items become the stale snapshot"
    );
}

// -- Offline gate -----------------------------------------------------------

#[test]
fn offline_blocks_the_debounced_request() {
    let mut session = SearchSession::new();
    let t0 = Instant::now();
    session.set_query("abba", t0);
    session.set_offline(true);
    let cmds = session.tick(t0 + Duration::from_millis(150));
    assert!(cmds.is_empty(), "offline must issue no request");
    assert_eq!(session.group(SearchKind::Track), &GroupState::Idle);

    // Coming back online, the same still-armed deadline fires.
    session.set_offline(false);
    let cmds = session.tick(t0 + Duration::from_millis(150));
    assert_eq!(cmds.len(), 1);
}

#[test]
fn an_in_flight_reply_is_discarded_while_offline() {
    let mut session = SearchSession::new();
    let t0 = Instant::now();
    session.set_query("abba", t0);
    let cmd = expect_one_search_command(session.tick(t0 + Duration::from_millis(150)));
    let SourceCommand::SearchCatalog { request_id, .. } = cmd else {
        unreachable!()
    };

    session.set_offline(true);
    session.apply_reply(request_id, Ok(full_page(3)));

    assert_eq!(
        session.group(SearchKind::Track),
        &GroupState::Pending,
        "a reply arriving while offline must be discarded, not applied"
    );
}
