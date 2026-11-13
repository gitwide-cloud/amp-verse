# Analysis queue access recovery (1.0.11)

The former empty-prefix Access denied message came from a queue-level operation,
not a named audio file. Queue-level failures are no longer counted as failed songs.
The worker waits between attempts and reports the shared catalog path. Stop remains
available while access is unavailable; completed metadata is retained.

Atomic catalog publication retries short PermissionDenied/WouldBlock failures,
including Windows/SMB share-denial windows. It retries the same atomic rename;
it never deletes the destination or writes over it in place. Persistent errors
still surface with the catalog path rather than being hidden or changing ACLs.

Shared-record contention keeps the job leased/deferred for retry instead of
permanently failing its audio. Existing failed jobs whose recorded reason was
shared-record contention are recovered when the queue opens. Actual decode errors
and silent-audio failures remain failed, avoiding endless retries of invalid files.
An idle wait prevents rapid worker restarts while remaining jobs are claimed.

Regression tests exercise atomic publication after real Windows handle denial,
injected short denial, immediate propagation of unrelated errors, deferred jobs,
legacy contention recovery and preservation of genuine audio failures.

Live verification: six legacy contention failures were restored to pending. After installing 1.0.11, completed jobs increased from 4903 to 4909 while genuine unreadable audio remained separately recorded. Playback was restored paused. Workspace checks, the Windows publication tests, two progress-counter regressions, strict Clippy and release builds passed.

