# Fluent strings for the Search and Library feature
# (004-search-and-library-browse, contracts/ui-surface.md §9).
#
# Seeded by Phase 1 (Setup) with a header comment only; Phase 2
# (Foundational, T027) adds the shared row/action keys every catalog list
# renders through (rows.rs). Search/Library/detail-specific keys are added
# by their own user-story phase (US1 T040, US2 T066) as those views land.

search-placeholder = Search
search-offline = Search needs a connection — you're offline
search-no-results = No results for “{ $query }” — check your spelling, or you might be offline.
search-group-tracks = Tracks
search-group-albums = Albums
search-group-artists = Artists
search-group-playlists = Playlists
search-show-more = Show more { $group }
refreshing = Refreshing…
row-explicit = Explicit
row-actions = Actions for { $name }
row-unavailable-region = Unavailable in your region
row-removed = Removed from the service
action-play-now = Play now
action-play-next = Play next
action-add-to-queue = Add to queue
action-add-to-playlist = Add to playlist
action-save-to-library = Save to library
action-pin-offline = Pin for offline
coming-soon = Coming soon
loading = Loading
detail-back = Back

# Library view (US2 T066, contracts/ui-surface.md §3/§9).
library-tab-saved-tracks = Saved Tracks
library-tab-saved-albums = Saved Albums
library-tab-followed-artists = Followed Artists
library-tab-playlists = Playlists
library-tab-recently-played = Recently Played
library-empty = Your library is empty — search to add music
library-empty-albums = Your saved albums are empty — search to add music
library-empty-artists = You're not following any artists yet — search to add music
library-empty-playlists = No playlists yet — create one
library-empty-recent = Nothing played yet — search to start listening
library-first-sync-failed = Couldn't load your library — check your connection
library-retry = Retry
action-search = Search
action-create = Create
playlist-owner = Owner: { $name }
playlist-no-tracks = This playlist has no tracks
playlist-track-count = { $count ->
    [one] 1 track
   *[other] { $count } tracks
}
