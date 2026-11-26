# Engine integration checkpoint

Integration branch: `feature/rust-engine-knowledge-integrated-20261119`. This combines committed engine work without modifying the original agent worktrees or live services.

Included: Rust knowledge-store/chroma/JSON-ID/cancellation fixes from `2e93a64`; native confidence/planning, loudness/session gain, desktop/Cast limiter, audio-clock handoffs, harmonic/tempo/energy/vocal work from SAG-69 (`7cb0571`, `b9f0ca1`, `b23340b`, `16e7b75`); reviewed bounded heartbeat fix `258e85f`; stable copy of the pending preparation-capacity cancellation patch (diff SHA-256 `51b2edcae33ea85fe3a7965a5dba7e6f9176676fab8ab55f04418461e27f1455`); launcher isolation `9c30a7e`.

The preparation patch acquires capacity before claiming a queued job, supports cancellation while waiting, observes changed limits, and releases capacity on cancellation. Its original source remains untouched in the agent worktree. It needs combined-build validation; earlier tests on the heartbeat commit alone do not certify this patch.

F04c numerical contract resolution is recorded separately in `tests/sag65/contract_resolutions.json`. Follow the explicit confidence-weighted formula: raw cost zero at confidence .85 gives .135. The original fixture's .1 maximum is preserved as historical input and explicitly not claimed to pass. This removes the technical clarification wait without inventing a user approval or changing source audio. Listening quality remains a separate acceptance task.

Required verification: Windows AND Linux full workspace tests/lint/release desktop/server builds, Android compilation for preserved JNI exports, generated fixture planner/metadata tests, measured desktop and Cast emit-path true peaks, native HLS lifecycle/recovery, preparation cancellation/heartbeat regressions, and isolated generated-audio executable smoke. Do not call integration complete based only on component or older-branch tests.

Existing a7 chroma component benchmarks (4.29x Windows /4.92x Linux) establish component throughput and bit parity, not total analysis time or this combined branch's audio quality. No physical listening, DJ controller, live external mixer deadline, or production migration is implied by automated checks.

UI redesign remains last. After combined engine gates, functionality work begins with normal/Smart Playlist identity/revision/sync and type-aware ordered/shuffle/Smart selection, followed by isolated station hosting and transport/integration capabilities. Mobile analysis writes remain prohibited; playlist edits are supported. Library management alone may organize files.
