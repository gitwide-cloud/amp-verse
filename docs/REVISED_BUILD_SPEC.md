# AMP⚡VERSE — Companion Integration & Smart Playlist Engine
**Revised Build Specification | AMP Library / AMP DJ / Companion**

## 1. Core Objective

Build a lightweight, intelligent Auto DJ system controlled through Bitfocus Companion.

**Companion directs the music. AMP DJ performs the music. AMP Library prepares the music.**

The operator can request songs, recognition music, background music, energy changes, and automatic transitions without manually choosing every song or operating the decks.

The music-analysis engine handles musical decisions using previously prepared metadata.

### Strict separation of responsibilities

- **AMP Library:** Music management, analysis, recommendations, metadata, cues, beatgrids, Smart Playlist configuration, and approval of suggested changes.
- **AMP DJ:** Live performance, Auto DJ, manual mixing, decks, effects, stems, performance pads, and crossfading.
- **Companion:** High-level music commands and playback feedback.
- **AMP Cast:** Station hosting, live broadcasting, and remote audio distribution.

Do not duplicate the Library's editing functions inside AMP DJ.

---

## 2. AMP Library — Music Preparation

AMP Library is the single workspace for preparing music before a performance.

### Library responsibilities

- Scan local folders and NAS directories.
- Manage folder-based music collections and playlists.
- Analyze BPM and beatgrids.
- Detect musical key and Camelot notation.
- Analyze overall and section-level energy.
- Identify intros, verses, builds, drops, breakdowns, and outros where possible.
- Detect phrase boundaries.
- Recommend cue-in and cue-out positions.
- Manage numbered hot cues and loops.
- Manage precomputed stems.
- Configure Smart Playlists and mixing preferences.
- Preview and approve suggested metadata changes.

Analysis is performed by the desktop Rust engine or AMP Cast background worker. Android reads the prepared metadata and supports local manual cue overrides without running heavyweight analysis.

### Analysis recommendations — user approval required

**Automatic analysis must never overwrite manually corrected cues, beatgrids, loops, or other user-edited metadata.**

When the analysis engine detects potentially better information, it should create a recommendation rather than applying a destructive change.

For example:

- Existing manual Cue 2: `01:12.000`
- Suggested Cue 2: `01:11.840`
- Reason: Better alignment with the detected phrase boundary.

The user can review the recommendation in AMP Library.

Available actions:

1. **Accept** — Apply the suggested change.
2. **Reject** — Discard the recommendation.
3. **Keep Existing** — Retain the current value without modifying the track.
4. **Preview** — Audition the proposed change before deciding.

Allow individual or bulk approval, with a review screen showing exactly which values will change.

Reanalysis may update previously unconfirmed automatic metadata, but it must preserve user-approved or manually edited values unless the user explicitly authorizes replacement.

Store accepted changes and user overrides in the portable `.ampverse` metadata directory alongside the music library.

### Shared metadata

The saved analysis should include:

| Data | Purpose |
|---|---|
| BPM / beatgrid | Beat synchronization |
| Musical key | Harmonic compatibility |
| Overall energy | Track selection |
| Section energy | Mixing and section selection |
| Phrase boundaries | Musical transitions |
| Cue-in / cue-out | Entry and exit timing |
| Hot cues | Manual and automated performance |
| Loops | Saved musical sections |
| Stem references | Prepared vocal, drum, bass, and instrumental audio |
| Analysis confidence | Determines safe fallback behavior |
| User overrides | Protects manually approved decisions |

**Analyze once. Review changes. Reuse everywhere.**

---

## 3. AMP Library — Smart Playlist Configuration

A Smart Playlist is a rule-based music collection whose playback decisions are handled by the analysis engine.

It is not simply a shuffled list.

The user creates and configures Smart Playlists in AMP Library.

### Smart Playlist settings

- Playlist name
- Source music folders or collections
- Allowed and excluded songs
- Mixing style
- Optional target energy
- Recent-repeat restrictions
- Preferred tracks
- Assigned recognition songs
- Manual cue preferences
- Optional energy progression
- Default fallback behavior

AMP Library saves these settings.

AMP DJ loads the saved configuration and performs the mix without requiring the operator to edit the playlist during showtime.

### Default Smart Playlists

| Playlist | Purpose | Suggested style |
|---|---|---|
| Recognition | Awards and milestones | Hype |
| Walk-In Music | Speaker entrances | Hype |
| Meeting Open | Opening music | Steady / Hype |
| Everyday Music | Continuous general listening | Steady |
| Background Music | Conversations and networking | Chill |
| Break Music | Intermissions | Steady |
| Closing Music | Meeting conclusion | Hype |
| Custom | User-defined collection | User selected |

All default configurations must be editable in AMP Library.

---

## 4. Smart Playlist Mixing Styles

The music-analysis engine automatically manages the selection of songs, sections, cues, and transitions according to the requested mixing style.

### HYPE

For recognition, celebrations, and energetic environments.

- Prioritize musical builds and drops.
- Play high-impact sections.
- Allow occasional intentional lulls.
- Use shorter song segments where appropriate.
- Favor dramatic but musically coherent transitions.
- Avoid unnecessarily long quiet intros and outros.

### STEADY

For continuous, consistent energy.

- Maintain a stable groove.
- Avoid abrupt energy drops.
- Select compatible songs and rhythmic sections.
- Favor smooth, phrase-aligned transitions.
- Avoid unnecessary dramatic effects.

### CHILL

For relaxed listening.

- Play most or nearly all of each song.
- Preserve the natural song structure.
- Allow quiet passages and breakdowns.
- Use gentle transitions.
- Avoid aggressive cue jumps.

### Mixing style versus energy

Mixing style determines how the music is performed.

Target energy determines the general intensity of the selection.

A high-energy Steady playlist can maintain a powerful groove without dramatic drops.

A low-energy Hype playlist can still feature builds and contrasts.

Both are managed automatically by the engine.

---

## 5. Progressive Energy

AMP DJ must support gradual changes in playlist energy.

The operator provides:

- Target energy: 1–10
- Number of song transitions
- Mixing style
- Hold target or resume normal playlist behavior

The engine uses the currently playing energy as the starting point unless explicitly overridden.

### Example — Wind Down

| Position | Target energy |
|---|---:|
| Current song | 9 |
| Transition 1 | 7 |
| Transition 2 | 5 |
| Transition 3 | 3 |

AMP DJ chooses compatible songs and transitions to approach the requested energy curve.

Do not stop or abruptly alter the current song.

If the target cannot be reached with available tracks, use the nearest suitable candidates.

### Supported commands

- Start Progressive Energy
- Cancel Progression
- Reverse Progression
- Hold Current Energy
- Change Target Energy

These commands alter the active performance plan, not the saved Library configuration.

---

## 6. AMP DJ — Showtime Only

AMP DJ is a performance workspace.

It is designed for live use, with minimal configuration required once the music is prepared.

### Auto DJ mode

AMP DJ automatically:

1. Loads the selected Smart Playlist.
2. Evaluates prepared analysis metadata.
3. Chooses the next suitable song.
4. Selects cue-in and cue-out positions.
5. Determines the transition method.
6. Preloads the incoming deck.
7. Performs the crossfade or prepared transition.
8. Continues according to the mixing style and energy plan.

Runtime decisions may vary within the user's saved rules.

**Runtime Auto DJ decisions must not rewrite the Library's approved analysis or permanent cue positions.**

### Manual DJ mode

The operator controls:

- Deck loading
- Play/pause
- Hot cues
- Loops
- Pitch and tempo
- Crossfader
- EQ
- Effects
- Sampler pads
- Stem levels

The Hercules controller, OSC, MIDI, and desktop controls can operate the same performance engine.

### AMP DJ must not provide

- Permanent beatgrid editing
- Permanent cue-point editing
- Music library rescanning
- Metadata approval workflows
- Smart Playlist rule editing
- Background-analysis configuration

If the operator notices an incorrect cue or beatgrid during a performance, AMP DJ may flag the track for review.

The correction itself is performed in AMP Library.

Any temporary performance cue adjustment must remain session-local unless the user explicitly saves it through the Library workflow.

---

## 7. Companion — Playback Actions

### CALL SONG

Parameters:

- Song ID or title
- Target deck: Auto / A / B
- Execution: Load Only / Play Now / Auto Fade

Default: **Load Only**.

Loading a song must not interrupt existing playback.

### CALL SONG + CUE

Parameters:

- Song ID
- Saved cue number or named cue
- Target deck
- Execution mode

Supports loading, immediate playback, or an automated transition from the requested cue.

### PRELOAD NEXT

Prepares the next track on an available deck without starting it.

### PLAY / PAUSE / STOP

Standard playback actions.

STOP is immediate and must remain separate from Fade Out.

### NEXT SONG

Requests the next track from the active playlist.

If Auto DJ is enabled, use a prepared musical transition.

### RESTART FROM CUE

Restarts the selected track from its assigned cue point.

---

## 8. Companion — Fades and Transitions

### FADE IN

Raises the selected deck or master volume smoothly.

Parameters:

- Duration
- Target level
- Deck or master

### FADE OUT

Lowers the selected deck or master volume smoothly.

Parameters:

- Duration
- Stop after fade: Yes / No

### AUTO FADE

Automatically transitions from the current song to the prepared next song.

The engine:

1. Identifies the playing deck.
2. Loads or confirms the next song.
3. Uses prepared cue and beatgrid metadata.
4. Starts the incoming song.
5. Operates the software crossfader.
6. Completes the transition.
7. Makes the incoming deck active.

### AUTO CROSSFADE

Enables or disables automatic crossfader operation.

### MIX NEXT

Requests the next suitable transition.

Provide an immediate-transition option for live use.

### DUCK / RESTORE

Temporarily lowers the music level and smoothly restores the previous level.

---

## 9. Companion — Smart Playlist Actions

### LOAD SMART PLAYLIST

Parameters:

- Playlist name or ID
- Load / Start / Transition

Optional runtime overrides:

- Mixing style
- Energy target
- Selection filters

The overrides affect the active performance without rewriting the saved playlist.

### RECOGNITION

Loads or starts the Recognition Smart Playlist.

Optional:

- Milestone
- Recipient
- Team
- Assigned song
- Assigned cue

Explicitly assigned songs take priority.

Otherwise, the engine selects suitable tracks automatically.

### EVERYDAY MUSIC

Starts the Everyday Music Smart Playlist.

Automatic song selection, mixing, and repeat avoidance continue until stopped.

### WALK-IN MUSIC

Loads the Walk-In Smart Playlist or a designated speaker track.

### BACKGROUND MUSIC

Starts the Background Music Smart Playlist, normally using Chill mode.

### SET MIXING STYLE

- Hype
- Steady
- Chill

Changes take effect at the next appropriate transition.

### SET ENERGY

Set target energy between 1 and 10.

### ENERGY + / ENERGY −

Incrementally change the target energy.

### PROGRESSIVE ENERGY

Start a controlled change toward the target energy over a specified number of transitions.

### SMART SHUFFLE

Request a different eligible next track while respecting the active rules.

---

## 10. Companion — Advanced DJ Actions

Available when AMP DJ is installed:

| Action | Function |
|---|---|
| Load Deck A/B | Load a specified song |
| Play/Pause Deck | Individual deck control |
| Trigger Hot Cue | Use a saved numbered cue |
| Set Crossfader | Control software crossfader |
| Enable/Disable Auto DJ | Change performance mode |
| Trigger FX Preset | Activate prepared effect |
| Trigger Sampler | Play a selected sample |
| Set Stem Level | Adjust prepared stems |
| Manual DJ Lock | Protect an active manual performance |
| Recall Music Scene | Apply a saved show-music configuration |

Advanced controls should reside on a secondary Companion page.

---

## 11. Music Scenes

Music Scenes allow Companion to recall a complete music configuration with one action.

A scene may contain:

- Smart Playlist
- Mixing style
- Target energy
- Auto DJ state
- Master output level
- Crossfade behavior
- Progressive Energy settings

Suggested scenes:

| Scene | Playlist | Style |
|---|---|---|
| Meeting Open | Meeting Open | Steady / Hype |
| Recognition | Recognition | Hype |
| Networking | Background Music | Chill |
| Meeting Break | Break Music | Steady |
| Closing | Closing Music | Hype |

Scenes can be configured in AMP Library or desktop settings.

Companion only recalls them.

---

## 12. Recommended Companion Layout

### Primary Page

| Row | Button 1 | Button 2 | Button 3 | Button 4 |
|---|---|---|---|---|
| Playback | Play | Pause | Stop | Next |
| Music | Call Song | Song + Cue | Recognition | Everyday |
| Transitions | Fade In | Fade Out | Auto Fade | Duck |
| Smart DJ | Hype | Steady | Chill | Progressive |
| Extras | Energy − | Energy + | Auto DJ | Preload |

Provide an optional simplified eight-button preset:

- Play
- Pause
- Fade Out
- Auto Fade
- Call Song
- Recognition
- Everyday
- Smart DJ

### Secondary Page

Advanced deck, FX, stem, sampler, MIDI, and manual performance actions.

---

## 13. Companion Feedback

Expose the actual state of the running engine.

| Variable | Meaning |
|---|---|
| `amp.connected` | Engine connection |
| `amp.playback_state` | Playing / Paused / Stopped |
| `amp.current_song` | Current title |
| `amp.current_artist` | Artist |
| `amp.current_cue` | Current performance cue |
| `amp.active_deck` | Playing deck |
| `amp.next_song` | Prepared incoming track |
| `amp.remaining_time` | Remaining playback time |
| `amp.playlist` | Active playlist |
| `amp.mix_style` | Hype / Steady / Chill |
| `amp.target_energy` | Requested energy |
| `amp.current_energy` | Estimated musical energy |
| `amp.progression_remaining` | Remaining progression transitions |
| `amp.auto_dj_enabled` | Auto DJ state |
| `amp.auto_crossfade_enabled` | Automatic crossfader state |
| `amp.fade_state` | Idle / Running |
| `amp.fade_progress` | Transition completion |
| `amp.master_level` | Output level |
| `amp.manual_dj_locked` | Manual performance protection |

Feedback must reflect actual engine state rather than merely confirming a command was sent.

---

## 14. Network API

Expose shared control functionality through:

- HTTP
- WebSocket state updates
- OSC
- MIDI mappings
- Bitfocus Companion integration

Use stable song IDs, playlist IDs, and cue IDs.

Support clear command acknowledgment and failure responses.

Prevent duplicate actions when a network command is retried.

Companion reconnecting must not restart or replay playback commands.

---

## 15. Office Automation Integration

Preserve the existing Aces United architecture:

**Node-RED → OnTime → Companion → AMP⚡VERSE**

Node-RED organizes event information and publishes schedules to OnTime.

OnTime provides timing and cues.

Companion handles show-control commands.

AMP⚡VERSE performs music selection and playback.

Neither Node-RED nor Companion needs to manage beatgrids, decide transitions, or choose musically compatible tracks.

Examples:

**Speaker walk-in:** Companion calls the designated song at its assigned cue.

**Recognition:** Companion starts or transitions to the Recognition Smart Playlist.

**Break:** Companion recalls the Break Music Scene.

**Meeting resumes:** Companion fades out or ducks music.

---

## 16. Reliability Rules

- Calling a song does not interrupt playing audio unless explicitly requested.
- Preload tracks before transitions whenever possible.
- Remote disconnections must not stop local playback.
- Companion reconnections must restore state without replaying commands.
- Manual DJ performances must remain protected from automated takeovers.
- User-approved cues, beatgrids, and metadata must never be automatically overwritten.
- Analysis changes are suggestions reviewed in AMP Library.
- AMP DJ runtime adjustments do not silently modify permanent metadata.
- Low-confidence musical analysis triggers safe fallback transitions.
- New energy targets apply at musically appropriate times.
- Background analysis must never compromise real-time playback.
- An unavailable network or server must not prevent prepared local music from playing.

---

## 17. Implementation Priorities

### Phase 1 — Show Control

- Call Song
- Call Song + Cue
- Preload
- Play/Pause/Stop
- Fade In/Fade Out
- Auto Fade
- Auto Crossfade
- Smart Playlist loading
- Playback feedback

### Phase 2 — Smart Music Engine

- Hype / Steady / Chill
- Energy-aware selection
- Section-aware cueing
- Recent-repeat avoidance
- Smart randomization
- Recognition / Everyday templates
- Progressive Energy
- Library recommendation and approval workflow

### Phase 3 — Advanced DJ

- Multi-deck hardware support
- MIDI/OSC mapping
- Sampler and FX
- Stem-level controls
- Manual DJ Lock
- Advanced Companion feedback

---

## 18. Acceptance Tests

1. A manually corrected cue remains unchanged after reanalysis.
2. New analysis suggestions appear in AMP Library and require user acceptance before replacing approved metadata.
3. Rejecting a suggested beatgrid change preserves the existing beatgrid.
4. AMP DJ can perform Auto DJ without offering permanent metadata-editing controls.
5. Runtime cue decisions do not overwrite stored user cue points.
6. Calling Song + Cue starts at the correct saved cue when instructed.
7. Load Only does not interrupt the playing track.
8. Auto Fade produces an uninterrupted transition.
9. Recognition chooses suitable eligible music and respects assigned tracks.
10. Hype emphasizes builds, drops, and intentional dynamics.
11. Steady maintains consistent energy where suitable music exists.
12. Chill preserves most of the original song structure.
13. Progressive Energy can transition from 9 to 3 over three songs without abrupt cuts.
14. Companion disconnection and reconnection do not interrupt or restart playback.
15. Duck and Restore correctly recover the prior volume.
16. Manual DJ Lock prevents automated takeover.
17. Show-control actions work on the local network without internet access.

## Final Principle

**AMP Library prepares and protects the music.**

**AMP DJ performs the music.**

**Companion controls the show.**

The engine may suggest changes to saved cues, beatgrids, and analysis, but only the user can approve those changes in AMP Library.

During showtime, AMP DJ concentrates exclusively on playback, automatic mixing, manual performance, and reliable transitions.
