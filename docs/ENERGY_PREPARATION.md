# Energy preparation and two-deck mixing

AMP Library embeds preparation above the song list. Select a song to see its full waveform, energy curve and sections. Select a section to rename it, adjust its energy, preview it or send that saved section into DJ. Sections can be split at the waveform cursor. BPM and beat-offset corrections remain explicit user edits.

The optional desktop analysis engine writes an energy profile into the portable catalog. RMS activity is measured on a common scale across songs; generated sections span 16 beats when tempo is available, otherwise eight seconds. These are neutral section boundaries, not verse/chorus recognition or inferred downbeats. Ambiguous tempo stays unknown. Half/double tempo alternatives are exposed for review.

AMP DJ retains two main decks. Auto DJ ranks eligible songs using their available entry-section energy, then chooses an incoming section near the outgoing handoff energy or requested target. Enough audio must remain for the fade. Explicit section requests and preferred manual Hype cues take priority. Manual cue bounds are protected; section mixing on manually edited tracks requires explicit opt-in. The saved fade duration is retained. Tempo matching requires known BPM on both tracks; this work does not add phase or phrase locking.

Edits use revision checks. Active decks retain their loaded snapshots. Reanalysis of protected preparation proposes the energy profile separately, allowing acceptance without replacing other corrections. No analysis engine is required merely to read prepared profiles.

Validation: workspace tests and strict Clippy; four real supplied MP3s analyzed with two coordinated workers; muted native two-deck handoff verified energy selection, tempo matching for the 88/89 BPM pair, continued Auto DJ, unchanged preparation revisions and unchanged source audio hashes. Two other supplied tracks retain unknown BPM and require review.
